# Design: Data-Oriented Parallel Arrays

**Status:** Proposed
**Pillars:** 5 (Scale Over Fidelity), 2 (Emergent Complexity)
**Phase:** Phase 3 prep (Scale)
**Depends on:** Spatial Hash Grid (uses the grid for queries; parallel arrays handle the per-entity AI iteration)
**Unlocks:** SIMD-friendly AI iteration, future Rayon parallelism, reduced hecs query overhead at 500+ pop

## Problem

The AI hot path in `system_ai` (src/ecs/systems.rs) iterates every creature entity each tick. For each entity, it performs multiple hecs component lookups:

```rust
// Phase 2-3 of system_ai, for every entity:
let creature = *world.get::<&Creature>(e).unwrap();      // archetype lookup
let behavior_state = world.get::<&Behavior>(e).unwrap().state;  // second lookup
let speed = world.get::<&Behavior>(e).unwrap().speed;           // third lookup (same component, repeated)
let Some(pos) = world.get::<&Position>(e).ok().map(|p| *p)     // fourth lookup
```

hecs 0.11 stores components in archetype tables. Each `world.get::<&T>(e)` does: entity metadata lookup -> archetype ID -> column index -> borrow check -> pointer dereference. This is fast for single lookups but the pattern above performs 3-4 separate `get` calls per entity per tick. At 500 villagers, that is 1500-2000 archetype lookups per tick just for reading AI inputs -- before any actual AI logic runs.

The deeper problem is **access pattern fragmentation**. hecs archetypes group entities by their component combination. A villager has ~6 components (Position, Velocity, Sprite, Creature, Behavior, sometimes CarriedResource). When iterating villagers, we only need 3-4 fields for the AI decision: `(x, y, species, hunger, sight_range, behavior_state, speed)`. But hecs stores all components of the archetype together. Reading `creature.hunger` pulls the full `Creature` struct (40 bytes) into cache even though we only need 8 bytes. Reading `behavior.state` pulls `BehaviorState` (24 bytes) plus `speed` (8 bytes).

At 30 villagers this does not matter. At 500+, iterating scattered archetype rows with unused fields pollutes the L1/L2 cache. The AI hot loop touches ~72 bytes per entity but only reads ~36 bytes of useful data. That is a 50% cache waste ratio on every iteration.

### What the hot path actually reads

Profile-guided extraction. These are the fields that `ai_villager`, `ai_predator`, and `ai_prey` read on **every tick** for **every entity**, before branching into behavior-specific logic:

| Field | Source component | Size | Used by |
|-------|-----------------|------|---------|
| `x` | Position | 8B (f64) | all AI: distance checks, spatial queries |
| `y` | Position | 8B (f64) | all AI: distance checks, spatial queries |
| `species` | Creature | 1B (enum) | system_ai: dispatch to correct AI function |
| `hunger` | Creature | 8B (f64) | all AI: eat/flee/hunt decisions |
| `sight_range` | Creature | 8B (f64) | all AI: spatial query radius |
| `home_x` | Creature | 8B (f64) | prey/villager: flee-home, sleep-at-hut |
| `home_y` | Creature | 8B (f64) | prey/villager: flee-home, sleep-at-hut |
| `state` | Behavior | 24B (enum) | all AI: current behavior determines transitions |
| `speed` | Behavior | 8B (f64) | all AI: movement calculation |

**Total hot-path payload per entity: 81 bytes.** Aligned to 88 bytes with padding.

Fields NOT needed on the hot path (skipped in the parallel arrays):
- `Velocity` (dx, dy): written as output, not read as input during the AI decision
- `Sprite` (ch, fg): rendering only
- `CarriedResource`: only checked in Hauling state (branch-specific, not every-tick)
- `Creature.home_x/home_y`: used by most entities but could be deferred to branch-specific reads if profiling shows it is cold

## Solution: Parallel Flat Arrays

Extract the hot-path fields into contiguous, parallel flat arrays at the start of each `system_ai` call. The arrays are indexed by a dense local index (0..N), not by hecs Entity ID. A separate mapping array connects local indices back to Entity IDs for writeback.

### Data layout

```rust
/// Dense parallel arrays for AI-hot-path data.
/// Rebuilt from hecs each tick. Read-only during AI iteration.
/// Outputs (new state, new velocity) are collected separately and written back.
pub struct AiArrays {
    pub len: usize,

    // --- Identity (for writeback) ---
    pub entities: Vec<Entity>,        // hecs entity ID, for writing results back

    // --- Position (read by every query, every tick) ---
    pub x: Vec<f64>,
    pub y: Vec<f64>,

    // --- Creature fields (read every tick for dispatch + decisions) ---
    pub species: Vec<Species>,
    pub hunger: Vec<f64>,
    pub sight_range: Vec<f64>,
    pub home_x: Vec<f64>,
    pub home_y: Vec<f64>,

    // --- Behavior fields (read every tick for state transitions) ---
    pub state: Vec<BehaviorState>,
    pub speed: Vec<f64>,
}
```

**Why parallel arrays instead of an array of structs (AoS)?** The classic data-oriented argument: not every AI branch reads every field. `ai_predator` never reads `home_x`/`home_y` (predators do not have homes). The species dispatch reads `species` for ALL entities but reads `hunger` only for the matched subset. Parallel arrays let the CPU prefetcher stream each field independently. An AoS layout (`Vec<AiEntry>`) would be simpler but waste cache on unused fields per branch -- the same problem we are solving by leaving hecs.

**Why not a single packed struct anyway?** At our target scale (500-1000 entities), the difference between SoA and AoS is measurable but not dramatic -- maybe 10-20% on the AI loop. We choose SoA because (a) it is the correct direction for future SIMD (SSE/AVX can process 4 f64s at once from a contiguous `Vec<f64>`), (b) it makes Rayon parallelism trivial (split index ranges, no aliasing), and (c) it matches the established data-oriented pattern used in production ECS implementations (like Bevy's table storage, Unity DOTS).

### Sync mechanism: extract from hecs each tick

```rust
impl AiArrays {
    /// Pre-allocate with expected capacity. Called once in Game::new().
    pub fn new(capacity: usize) -> Self {
        AiArrays {
            len: 0,
            entities: Vec::with_capacity(capacity),
            x: Vec::with_capacity(capacity),
            y: Vec::with_capacity(capacity),
            species: Vec::with_capacity(capacity),
            hunger: Vec::with_capacity(capacity),
            sight_range: Vec::with_capacity(capacity),
            home_x: Vec::with_capacity(capacity),
            home_y: Vec::with_capacity(capacity),
            state: Vec::with_capacity(capacity),
            speed: Vec::with_capacity(capacity),
        }
    }

    /// Clear all arrays but retain allocated capacity.
    pub fn clear(&mut self) {
        self.len = 0;
        self.entities.clear();
        self.x.clear();
        self.y.clear();
        self.species.clear();
        self.hunger.clear();
        self.sight_range.clear();
        self.home_x.clear();
        self.home_y.clear();
        self.state.clear();
        self.speed.clear();
    }

    /// Extract hot-path fields from hecs in a single query pass.
    /// Replaces the Phase 2 entity collection and per-entity get() calls.
    pub fn extract_from_world(&mut self, world: &World) {
        self.clear();

        // Single query: all entities with the AI-relevant component set.
        // hecs 0.11: query iterates one archetype table at a time, so
        // entities with identical component sets are already contiguous.
        for (entity, (pos, creature, behavior)) in
            world.query::<(Entity, &Position, &Creature, &Behavior)>().iter()
        {
            self.entities.push(entity);
            self.x.push(pos.x);
            self.y.push(pos.y);
            self.species.push(creature.species);
            self.hunger.push(creature.hunger);
            self.sight_range.push(creature.sight_range);
            self.home_x.push(creature.home_x);
            self.home_y.push(creature.home_y);
            self.state.push(behavior.state);
            self.speed.push(behavior.speed);
        }

        self.len = self.entities.len();
    }
}
```

**One query instead of many.** The current code does `world.query::<(Entity, &Behavior)>` to collect entity IDs, then 3-4 individual `world.get` calls per entity. The extract pass does a single `world.query::<(Entity, &Position, &Creature, &Behavior)>` -- one archetype traversal, one borrow check, all fields read in one cache-hot pass through the archetype table.

**hecs 0.11 query semantics.** `world.query::<(Entity, &T1, &T2, &T3)>().iter()` returns shared borrows. This is compatible with the extract pattern because we only read during extraction. The `&World` borrow is released before the AI loop mutates anything.

### AI loop with parallel arrays

```rust
// In system_ai, replacing Phase 2-3:

ai_arrays.extract_from_world(world);

// Collect AI outputs into a results buffer
let mut results: Vec<AiOutput> = Vec::with_capacity(ai_arrays.len);

for i in 0..ai_arrays.len {
    // All reads from contiguous arrays -- no hecs lookups
    let species = ai_arrays.species[i];
    let pos_x = ai_arrays.x[i];
    let pos_y = ai_arrays.y[i];
    let hunger = ai_arrays.hunger[i];
    let sight = ai_arrays.sight_range[i];
    let state = ai_arrays.state[i];
    let spd = ai_arrays.speed[i];

    let output = match species {
        Species::Prey => ai_prey_from_arrays(i, &ai_arrays, grid, map, &mut rng),
        Species::Predator => ai_predator_from_arrays(i, &ai_arrays, grid, map, &mut rng),
        Species::Villager => ai_villager_from_arrays(i, &ai_arrays, grid, map, ...),
    };

    results.push(output);
}

// Phase 4: write results back to hecs
for (i, output) in results.iter().enumerate() {
    let e = ai_arrays.entities[i];
    if let Ok(mut behavior) = world.get::<&mut Behavior>(e) {
        behavior.state = output.new_state;
    }
    if let Ok(mut vel) = world.get::<&mut Velocity>(e) {
        vel.dx = output.new_vx;
        vel.dy = output.new_vy;
    }
    if let Ok(mut creature) = world.get::<&mut Creature>(e) {
        creature.hunger = output.new_hunger;
    }
}
```

**Separation of read and write.** The current code reads from hecs and writes back immediately per entity. The parallel array approach collects all reads up front and all writes at the end. This is the key enabler for future Rayon parallelism -- the AI loop becomes embarrassingly parallel because no entity reads another entity's in-progress output.

### AI output struct

```rust
/// Result of one entity's AI tick. Minimal: only the fields that AI can change.
pub struct AiOutput {
    pub new_state: BehaviorState,
    pub new_vx: f64,
    pub new_vy: f64,
    pub new_hunger: f64,
    pub deposited: Option<ResourceType>,
    pub claim_site: Option<Entity>,
}
```

This is 56 bytes per entity. At 500 entities: 28KB for the output buffer. Fits in L1 cache.

## Expected cache behavior improvement

### Current layout: hecs archetype iteration

A villager archetype row contains: Position (16B) + Velocity (16B) + Sprite (8B) + Creature (40B) + Behavior (32B) + possible CarriedResource (8B) = **~120 bytes per entity row**.

The AI hot path reads: pos.x, pos.y (16B) + creature.species, hunger, sight_range, home_x, home_y (33B padded to 40B) + behavior.state, speed (32B) = **88 bytes read out of 120 loaded**. That is a 73% utilization rate on archetype iteration.

But `system_ai` does NOT iterate the archetype directly. It collects Entity IDs first, then does individual `world.get` calls which each re-traverse the archetype metadata. Each `get` has overhead beyond the data read: entity metadata lookup (generation check), archetype column resolution, dynamic borrow tracking. At 4 gets per entity x 500 entities = 2000 dynamic borrow checks per tick.

### Parallel array layout

All hot-path data packed into 10 contiguous arrays:

| Array | Element size | 500 entities | Cache lines (64B) |
|-------|-------------|-------------|-------------------|
| entities | 8B | 4KB | 63 |
| x | 8B | 4KB | 63 |
| y | 8B | 4KB | 63 |
| species | 1B | 500B | 8 |
| hunger | 8B | 4KB | 63 |
| sight_range | 8B | 4KB | 63 |
| home_x | 8B | 4KB | 63 |
| home_y | 8B | 4KB | 63 |
| state | 24B | 12KB | 188 |
| speed | 8B | 4KB | 63 |
| **Total** | | **~44KB** | **~700** |

44KB fits comfortably in L2 cache (256KB on most CPUs). The species dispatch loop streams sequentially through the `species` array (500B = 8 cache lines), which the hardware prefetcher handles perfectly. Branch-specific arrays (like `home_x`/`home_y` only used by prey and villagers) are skipped entirely for predators -- no cache pollution.

**Concrete improvement:** Eliminating the 2000 dynamic borrow checks and replacing scattered archetype access with sequential array reads. Expected speedup on the AI read path: **2-3x at 500 pop**. The write-back path still uses individual `world.get::<&mut T>` calls, but that is 3 writes per entity instead of 4 reads+writes, and writes are less latency-sensitive.

### Why not just use a single hecs query?

A valid alternative is:

```rust
for (entity, (pos, creature, behavior, vel)) in
    world.query_mut::<(Entity, &Position, &mut Creature, &mut Behavior, &mut Velocity)>()
{
    // all fields available, no separate get() calls
}
```

This would fix the repeated-get problem and is worth doing as a first step. But it does NOT fix:

1. **Cache utilization.** hecs archetype rows still contain Sprite, CarriedResource, etc. The CPU loads full rows even though AI does not read rendering data.
2. **Read-write aliasing.** `query_mut` takes an exclusive borrow on the World. No other system can read the World during AI iteration. Parallel arrays copy data out, release the borrow, and allow the AI loop to be parallelized with Rayon later.
3. **Profile-guided extraction.** Parallel arrays let us choose exactly which fields to extract. If profiling shows `home_x`/`home_y` are cold (only 30% of entities use them), we can drop them from the arrays and load them on-demand from hecs. Archetype storage does not offer this flexibility.

The recommended approach: first consolidate the hecs queries into a single `query_mut` (cheap win, do it now), then extract into parallel arrays when profiling confirms the hot path at 200+ entities.

## Interaction with Spatial Hash Grid

The spatial hash grid and parallel arrays serve complementary roles:

- **Spatial hash grid:** Answers "which entities are near position X?" Replaces O(N) linear scans with O(nearby) lookups. Indexes all entity types by position and category.
- **Parallel arrays:** Answers "what are entity i's AI-relevant fields?" Replaces scattered hecs lookups with sequential array reads. Only contains creatures (entities with Behavior + Creature).

The grid is populated from a `world.query::<(Entity, &Position, ...)>` pass. The parallel arrays are populated from a `world.query::<(Entity, &Position, &Creature, &Behavior)>` pass. These could be fused into a single pass that populates both structures, but keeping them separate is cleaner and the grid includes non-creature entities (FoodSource, Stockpile, etc.) that do not belong in the AI arrays.

The AI loop reads entity fields from the parallel arrays and performs spatial lookups through the grid:

```rust
for i in 0..ai_arrays.len {
    let sight = ai_arrays.sight_range[i];
    let px = ai_arrays.x[i];
    let py = ai_arrays.y[i];

    // Spatial query through grid (nearby entities)
    let nearest_food = grid.nearest(px, py, sight, category::FOOD_SOURCE);

    // AI state from parallel arrays (this entity's own data)
    let state = ai_arrays.state[i];
    let hunger = ai_arrays.hunger[i];

    // ... decision logic using both
}
```

## What NOT to extract

Resist the temptation to put everything in parallel arrays. Only extract fields that are read on **every iteration** of the hot loop. Fields that are read conditionally (inside a specific behavior branch) should stay in hecs and be loaded on-demand via `world.get`.

| Field | Extract? | Reason |
|-------|----------|--------|
| Position (x, y) | Yes | Every entity, every tick |
| Species | Yes | Dispatch key, every entity |
| Hunger | Yes | Every entity checks hunger |
| Sight range | Yes | Every entity does spatial queries |
| Home (x, y) | Yes | Most entities (prey flee, villagers sleep) |
| Behavior state | Yes | Every entity, drives transitions |
| Speed | Yes | Every entity, movement output |
| CarriedResource | **No** | Only Hauling villagers (~15% of pop) |
| Sprite | **No** | Rendering only |
| Velocity | **No** | Output only, never read during AI decision |
| FarmPlot fields | **No** | Only farming villagers, only when at farm |
| BuildSite fields | **No** | Only building villagers, loaded via Entity ID from grid |

If profiling later shows that `home_x`/`home_y` are cold (predators do not use them, idle villagers do not use them), they can be dropped from the arrays. Start with the full set above and measure.

## Implementation plan

### Step 1: Consolidate hecs queries (immediate, low-risk)

Replace the Phase 2 entity collection + per-entity `world.get` calls with a single `world.query` that reads all AI-relevant components in one pass. This is a pure refactor that eliminates repeated archetype lookups without introducing new data structures.

```rust
// BEFORE: collect IDs, then get() per entity
let entities: Vec<Entity> = world.query::<(Entity, &Behavior)>().iter().map(|(e,_)| e).collect();
for e in entities {
    let creature = *world.get::<&Creature>(e).unwrap();
    let behavior_state = world.get::<&Behavior>(e).unwrap().state;
    // ...
}

// AFTER: single query, destructure everything at once
let snapshots: Vec<(Entity, Position, Creature, BehaviorState, f64)> = world
    .query::<(Entity, &Position, &Creature, &Behavior)>()
    .iter()
    .map(|(e, p, c, b)| (e, *p, *c, b.state, b.speed))
    .collect();
for (e, pos, creature, state, speed) in &snapshots {
    // ... use directly, no get() calls
}
```

Run test suite. Benchmark at current pop (30). This alone should show a measurable reduction in AI tick time.

### Step 2: Introduce AiArrays struct

Add `src/ecs/ai_arrays.rs` with the `AiArrays` struct, `extract_from_world`, and `clear`. Add a field to `Game`. Unit tests:

- Extract from world with 0 entities -> len == 0
- Extract from world with 10 creatures -> len == 10, fields match
- Extract skips entities without Creature component (FoodSource, Stockpile)
- Clear retains capacity (measure with `Vec::capacity`)
- Extract twice overwrites previous data correctly

### Step 3: Wire into system_ai

Replace the Phase 2 snapshot code with `ai_arrays.extract_from_world(world)`. The AI loop reads from `ai_arrays` instead of individual `world.get` calls. Outputs collected into `Vec<AiOutput>` and written back in a final pass.

Migration order:
1. Extract + read from arrays for species dispatch only (keep existing per-entity `get` for everything else)
2. Move position reads to arrays
3. Move creature field reads to arrays
4. Move behavior state reads to arrays
5. Introduce AiOutput, collect results, batch writeback

Each step: run full test suite, verify behavior unchanged.

### Step 4: Profile and prune

With arrays in place, run benchmarks at 100, 200, 500 entities (using headless mode with `--ticks`). Profile with `cargo flamegraph` or `cargo bench`:

- Is extract_from_world cheap? (Expected: <50us at 500 pop)
- Is the AI loop faster? (Expected: 2-3x on read path)
- Is writeback the new bottleneck? (If so, consider batch `world.query_mut` for writeback)
- Are `home_x`/`home_y` actually hot? (Check branch frequency in villager AI)

Prune arrays based on profiling data. Add fields only if they show up as hot. Remove fields if they are branch-cold.

### Step 5 (future): Rayon parallelism

With reads from arrays and writes to `Vec<AiOutput>`, the AI loop is embarrassingly parallel:

```rust
use rayon::prelude::*;

let results: Vec<AiOutput> = (0..ai_arrays.len)
    .into_par_iter()
    .map(|i| {
        match ai_arrays.species[i] {
            Species::Prey => ai_prey_from_arrays(i, &ai_arrays, &grid, map),
            Species::Predator => ai_predator_from_arrays(i, &ai_arrays, &grid, map),
            Species::Villager => ai_villager_from_arrays(i, &ai_arrays, &grid, map, ...),
        }
    })
    .collect();
```

This requires: (a) `&SpatialHashGrid` is `Sync` (it is read-only after population), (b) `&TileMap` is `Sync` (read-only), (c) RNG is per-thread (use `rayon::current_thread_index()` to seed). The main obstacle is that `ai_villager` currently takes `&mut rng` -- this needs to become a thread-local RNG in the parallel version.

Not in scope for this design. Listed here to show that parallel arrays are the foundation.

## Design decisions and trade-offs

**Rebuild every tick vs. persistent arrays with delta tracking.**
Same reasoning as the spatial hash grid: rebuild is simpler and correct by construction. At 500 entities, extracting 10 fields = 5000 field copies = ~10us. Entities move and change state every tick anyway, so delta tracking would update most entries regardless. Rebuild eliminates stale-data bugs.

**SoA (struct of arrays) vs. AoS (array of structs).**
SoA chosen for cache efficiency on partial reads and future SIMD compatibility. The cost is more verbose code (10 parallel Vecs instead of 1 Vec of structs). If the verbosity becomes a maintenance burden, a macro could generate the boilerplate, but at 10 fields it is manageable by hand.

**Parallel arrays live outside hecs, not as a custom storage layer.**
hecs 0.11 does not support custom column storage or SoA layouts. We could switch to an ECS that does (Bevy ECS, Flecs), but that is a massive migration for a targeted optimization. External parallel arrays are surgical: they solve the hot-path problem without touching the rest of the codebase.

**AiOutput as a flat struct, not written directly to arrays.**
Writing directly to output arrays (`new_vx: Vec<f64>`, etc.) would avoid the AiOutput struct allocation. But the struct is clearer, and 56 bytes x 500 = 28KB fits in L1. If writeback becomes a bottleneck, switching to output arrays is straightforward.

## File placement

```
src/ecs/
  ai_arrays.rs     # AiArrays struct, AiOutput, extract_from_world, tests
  mod.rs           # add `pub mod ai_arrays;` and re-export
```

## Future extensions

These are NOT part of this design but are enabled by it:

- **Rayon parallel AI:** The read-only arrays + write-to-output pattern is the textbook setup for data-parallel iteration. Swap `for i in 0..N` with `(0..N).into_par_iter().map(...)`.
- **SIMD distance checks:** `x` and `y` as contiguous `Vec<f64>` can be processed 4-at-a-time with AVX2 `_mm256_sub_pd` / `_mm256_mul_pd` for batch distance calculations in spatial queries.
- **Tick budgeting integration:** The parallel arrays make it trivial to skip entities: just iterate a subset of indices. Combined with tick budgeting, offscreen entities get lower-frequency AI by skipping their indices on non-update ticks.
- **Hot/cold split:** If profiling shows some fields are only read by 20% of entities, split into "hot arrays" (always extracted) and "cold arrays" (extracted on demand). The hot arrays shrink, fitting more entities in L1.
- **Archetype-aware extraction:** hecs 0.11 iterates archetypes internally. If we know all villagers share one archetype, we could `memcpy` the Position column directly into the `x`/`y` arrays. This requires unsafe code and knowledge of hecs internals -- not worth it now, but possible if extraction ever becomes the bottleneck.
