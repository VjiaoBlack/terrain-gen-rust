# Tick Budgeting

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 5 (Scale Over Fidelity)*

## Problem

`system_ai` in `src/ecs/systems.rs` iterates every entity with a `Behavior` component every single tick. For each entity it:

1. Snapshots all food, prey, predator, villager, stockpile, build site, stone deposit, and hut positions (O(entities) per category).
2. Collects every entity with `Behavior` into a `Vec<Entity>`.
3. Runs the full AI decision function (`ai_villager`, `ai_predator`, `ai_prey`) for each one, including sight-range distance checks against the snapshots and A* pathfinding calls.

At 30 villagers this is negligible. At 500 it dominates the frame budget. The game design doc targets 500 villagers at 60fps with a 5ms AI budget per tick. Running 500 full AI evaluations per tick will not fit in 5ms.

Most of this work is wasted. A villager sleeping in a hut does not need to re-evaluate its AI every tick -- it just needs to count down its sleep timer. A villager gathering wood 80 tiles offscreen does not need to run every tick either. Only villagers in active, time-sensitive situations (fleeing a predator, near the camera, hauling near a stockpile) need per-tick updates.

## Solution: Priority-Based Tick Budgeting

Assign each entity an **AI priority category** that determines how often its full AI function runs. Between scheduled ticks, the entity continues its current behavior (timers count down, movement continues along current velocity) but does not re-evaluate decisions.

### Priority Categories

| Category | Tick Interval | Who | Why |
|----------|--------------|-----|-----|
| **Critical** | Every tick (1) | Fleeing, under attack, captured, hunting near target | Life-or-death decisions cannot be delayed |
| **Active** | Every 2 ticks | Hauling, building, farming, working, gathering (timer-driven activities near completion) | Productive work with tight timing; small delay is acceptable |
| **Normal** | Every 3-4 ticks | Wandering, seeking, exploring, eating | Moving toward a goal; rechecking every 3 ticks is plenty |
| **Idle** | Every 6-8 ticks | Idle (counting down timer), sleeping | Not doing anything meaningful; just waiting for timer |
| **Dormant** | Every 10-15 ticks | Offscreen AND (idle or sleeping) | Player cannot see them; no gameplay-visible difference |

### Category Assignment Rules

A villager's category is determined by its `BehaviorState` and world context. Evaluated when:
- The entity's AI runs (re-categorize at end of each AI tick).
- An **interrupt** fires (see below).

```
fn tick_priority(state: &BehaviorState, predator_nearby: bool, onscreen: bool) -> TickPriority {
    // Threat always overrides everything
    if predator_nearby {
        return TickPriority::Critical;
    }

    match state {
        // Life-or-death
        FleeHome { .. } | Hunting { .. } | Captured => TickPriority::Critical,

        // Active productive work
        Hauling { .. } | Building { .. } | Gathering { timer, .. } if *timer <= 5 => {
            TickPriority::Active
        }
        Gathering { .. } | Farming { .. } | Working { .. } => TickPriority::Active,

        // Goal-directed movement
        Seek { .. } | Wander { .. } | Exploring { .. } | Eating { .. } => {
            TickPriority::Normal
        }

        // Waiting
        Idle { .. } | Sleeping { .. } => {
            if onscreen { TickPriority::Idle } else { TickPriority::Dormant }
        }
    }
}
```

The `onscreen` check uses the camera viewport: is the entity's position within the visible tile range plus a small margin (e.g., 5 tiles)? This is a cheap integer bounds check, not a per-entity distance calculation.

### Scheduling Mechanism

Add a `TickSchedule` component (or field on `Behavior`):

```rust
pub struct TickSchedule {
    /// Which tick this entity next runs AI. Set after each AI evaluation.
    pub next_ai_tick: u64,
    /// Current priority category (cached for diagnostics/debugging).
    pub priority: TickPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickPriority {
    Critical,  // interval 1
    Active,    // interval 2
    Normal,    // interval 3-4
    Idle,      // interval 6-8
    Dormant,   // interval 10-15
}
```

In `system_ai`, the entity loop changes from:

```rust
// BEFORE: process every entity every tick
for e in entities { ... }
```

to:

```rust
// AFTER: only process entities scheduled for this tick
for e in entities {
    let schedule = world.get::<&TickSchedule>(e);
    if schedule.next_ai_tick > current_tick {
        continue; // not scheduled yet -- keep current velocity/state
    }
    // ... run full AI ...
    // After deciding new state:
    let priority = tick_priority(&new_state, predator_nearby, onscreen);
    let interval = priority.interval(rng); // randomized within range
    schedule.next_ai_tick = current_tick + interval;
    schedule.priority = priority;
}
```

The interval is **randomized within the category's range** (e.g., Normal picks 3 or 4 at random). This prevents all Normal-priority entities from synchronizing on the same tick, which would create frame-time spikes. Staggering distributes the load evenly.

### Between AI Ticks: What Continues

When an entity is skipped because it is not scheduled:

- **Movement continues.** `system_movement` still runs for all entities every tick. Velocity was set during the last AI tick and persists. A villager walking to a stockpile keeps walking.
- **Timers count down.** Hunger ticks via `system_hunger` every tick (cheap, no branching). Sleep/idle/gather timers are part of `BehaviorState` and only decrement inside the AI function, so they effectively run at the entity's tick interval. This is acceptable: a sleeping villager waking up 5 ticks "late" is invisible to the player.
- **Rendering is unaffected.** Entities are drawn at their current position every frame regardless of AI schedule. The player sees smooth movement.

### Interrupts: Handling Urgent State Transitions

The critical design challenge: a Dormant villager sleeping offscreen has `next_ai_tick` set 15 ticks in the future. A wolf pack moves into its area at tick N+3. The villager must react, not sleep through its own death.

**Solution: Threat Interrupt System.**

Interrupts are events that force an entity to run AI next tick regardless of schedule. They are checked cheaply (no full AI evaluation).

#### Interrupt Sources

| Interrupt | Trigger | Cost |
|-----------|---------|------|
| **Predator enters range** | During predator AI, when a predator moves within threat range (8 tiles) of any entity, mark nearby entities for interrupt | O(predators * nearby_entities) -- tiny with spatial grid |
| **Hunger critical** | `system_hunger` checks if hunger > 0.85; if so, set `next_ai_tick = current_tick` | O(1) per entity, checked every tick already |
| **State timer expired** | When a behavior timer hits 0, the entity needs to decide what to do next | Checked inline during timer decrement |
| **Camera entered view** | When camera moves, entities newly onscreen get promoted from Dormant to Idle | O(newly_visible_entities) per camera move |

Implementation: interrupts set `next_ai_tick = current_tick` (or `current_tick + 1`) on the affected entity. No separate interrupt queue needed. The existing scheduling check picks it up naturally.

```rust
// In system_hunger (already iterates all creatures):
pub fn system_hunger(world: &mut World, hunger_mult: f64, current_tick: u64) {
    for (creature, schedule) in world.query_mut::<(&mut Creature, &mut TickSchedule)>() {
        // ... existing hunger increment ...
        if creature.hunger > 0.85 && schedule.next_ai_tick > current_tick + 1 {
            schedule.next_ai_tick = current_tick + 1; // interrupt: need to find food NOW
        }
    }
}
```

For predator proximity interrupts, the cheapest approach before spatial grids exist:

```rust
// After predator AI runs and updates predator positions, check each predator
// against all entities in a rough bounding box. This is O(predators * entities)
// but predator count is small (5-20), so it's fast.
for &(px, py) in &predator_positions {
    for (pos, schedule) in world.query_mut::<(&Position, &mut TickSchedule)>() {
        if dist(pos.x, pos.y, px, py) < 10.0 && schedule.next_ai_tick > current_tick + 1 {
            schedule.next_ai_tick = current_tick + 1;
        }
    }
}
```

Once a spatial hash grid exists (a separate Pillar 5 feature), this becomes O(predators * entities_per_cell), which is near-free.

### Phase 1 Snapshot Optimization

The entity snapshot phase at the top of `system_ai` (lines 100-160 in systems.rs) collects ALL positions of food, prey, predators, stockpiles, build sites, stone deposits, and huts into Vecs every tick. This is wasteful when most entities are skipped.

**Short-term fix:** The snapshots are still needed for the entities that DO run this tick. Keep the snapshot phase but accept it as a known cost. At 500 entities, 7 snapshot passes is ~3500 iterations -- under 0.1ms.

**Medium-term fix:** Move snapshots to a cached structure that updates incrementally. Positions change every tick (from movement), but most other data (stockpile locations, build sites, stone deposits) changes rarely. Cache these and only rebuild when a building is placed/destroyed or a deposit is created/depleted. Only position snapshots need per-tick refresh.

**Long-term fix:** Spatial hash grid replaces all snapshot Vecs. "Find food near me" becomes a grid cell lookup. Snapshots disappear entirely.

### Expected Performance Impact

**Conservative estimate at 500 villagers:**

Current: 500 AI evaluations per tick.

With tick budgeting, assuming a typical population distribution:
- ~20 Critical (fleeing, hunting): 20 evals/tick
- ~80 Active (gathering, building, farming): 40 evals/tick (half run each tick)
- ~150 Normal (wandering, seeking): 42 evals/tick (one-third run each tick)
- ~100 Idle (waiting): 14 evals/tick (one-seventh run each tick)
- ~150 Dormant (sleeping offscreen, idle offscreen): 12 evals/tick (one-twelfth run each tick)

**Total: ~128 evals/tick instead of 500. Roughly 3.9x reduction.**

The distribution skews better in practice because large settlements have many sleepers at night and many idle villagers during peaceful periods. During a wolf raid, more entities go Critical, but raids are brief and the spike is acceptable (the player is watching the action, so per-tick AI is actually valuable then).

### Integration with Existing Systems

#### system_movement -- no change
Runs every tick for all entities. Cheap (position + velocity arithmetic). Entities between AI ticks continue moving at their last-set velocity.

#### system_hunger -- minor change
Already iterates all creatures every tick. Add the hunger-critical interrupt check (one comparison per entity). Pass `current_tick` as a new parameter.

#### system_death -- no change
Checks `hunger >= 1.0` for all creatures. Cheap, must remain per-tick.

#### system_farms -- no change
Iterates farm plots, not villagers. Unrelated to tick budgeting.

#### system_breeding -- no change
Runs every tick but only spawns on specific intervals. Unrelated.

#### Game::step() call site
Pass `current_tick` (already available as `self.tick`) to `system_ai`. The scheduling logic lives inside `system_ai`, so `Game::step()` needs minimal changes.

### New Component: TickSchedule

Added to every entity that has a `Behavior` component. Spawned with `next_ai_tick: 0` (run immediately on first tick) and `priority: Normal`.

Serialization: `TickSchedule` must be included in save/load (`src/ecs/serialize.rs`). On load, set all `next_ai_tick` to 0 so entities re-evaluate immediately.

### Diagnostics

Add to `collect_diagnostics()` in `game/mod.rs`:
- Count of entities per `TickPriority` category.
- Number of AI evaluations that actually ran this tick.
- Running average of AI evals per tick over the last 100 ticks.

This lets us verify the 3-5x reduction claim and tune intervals.

## Implementation Plan

### Step 1: Add TickSchedule component and priority function
- Add `TickSchedule` and `TickPriority` to `src/ecs/components.rs`.
- Add `tick_priority()` function to `src/ecs/ai.rs`.
- Add `TickSchedule` to spawn functions in `src/ecs/spawn.rs`.
- Add `TickSchedule` to serialization in `src/ecs/serialize.rs`.

### Step 2: Gate system_ai with schedule check
- Modify the entity loop in `system_ai` to skip entities where `next_ai_tick > current_tick`.
- After AI evaluation, compute priority and set `next_ai_tick`.
- Pass `current_tick` from `Game::step()`.

### Step 3: Add interrupt checks
- Hunger interrupt in `system_hunger`.
- Predator proximity interrupt after predator AI (inside `system_ai` or as a post-pass).
- Timer-expiry interrupt (entities whose behavior timer is about to hit 0 get scheduled).

### Step 4: Add diagnostics
- Priority distribution counts in `collect_diagnostics()`.
- AI evals/tick counter.

### Step 5: Tune and validate
- Run 30K tick simulations at pop 30, compare behavior to pre-budgeting baseline.
- Verify no visible behavioral differences (settlement shape, resource curves, population growth should be statistically identical).
- Profile at simulated pop 200+ (spawn extra villagers) and measure actual frame time reduction.

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Timer-driven states (Gathering, Sleeping) tick at lower frequency, causing timing drift | Acceptable: a few ticks of drift is invisible. If needed, decrement timers proportionally to interval (`timer -= interval` instead of `timer -= 1`). |
| Dormant entities pile up state changes when they finally run | AI function already handles any state cleanly -- it reads current state and decides next action. No accumulated queue. |
| Predator interrupt check is O(predators * entities) before spatial grid | Predator count is small (5-20). At 500 entities this is 2500-10000 distance checks per tick, well under 0.1ms. Acceptable until spatial grid lands. |
| Velocity persists between AI ticks, causing entities to walk past destinations | `system_movement` handles collision/bouncing. Overshooting by 1-3 tiles is invisible for Normal/Idle entities. For Active entities (interval 2), overshoot is at most 1 tile. |
| Save/load with stale TickSchedule values | Reset all `next_ai_tick` to 0 on load. |

## Open Questions

- Should predators and prey also use tick budgeting, or only villagers? Predators are few (5-20), so budgeting them saves little. Prey are more numerous (20-50) and mostly wander -- good candidates for Normal/Idle budgeting.
- Should the tick interval scale with total entity count? At 100 entities, tighter intervals (less skipping) preserve responsiveness. At 1000, wider intervals are needed to fit budget. An adaptive system could adjust intervals based on measured frame time.
- Should onscreen detection use the current camera position or a slightly stale one? Using stale (updated every 10 ticks) avoids rechecking viewport bounds every tick, but rapid scrolling could leave entities Dormant while visible for a few frames.

## References

- `src/ecs/systems.rs` -- `system_ai` (lines 75-450), `system_hunger`, `system_movement`
- `src/ecs/ai.rs` -- `ai_villager`, `ai_predator`, `ai_prey`, `tick_priority` (proposed)
- `src/ecs/components.rs` -- `BehaviorState`, `Behavior`, `TickSchedule` (proposed)
- `src/game/mod.rs` -- `Game::step()` (lines 1080-1210), `collect_diagnostics()`
- `docs/game_design.md` -- Pillar 5: Scale Over Fidelity
