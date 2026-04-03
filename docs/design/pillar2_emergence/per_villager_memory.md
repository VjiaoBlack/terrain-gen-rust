# Per-Villager Memory

**Status:** Proposed
**Pillar:** 2 (Emergent Complexity from Simple Agents)
**Phase:** Foundation / Economy Depth boundary
**Depends on:** None (can be built incrementally on current ECS)
**Enables:** Bulletin board (Layer 3), exploration-as-discovery, misinformation/stale-knowledge emergent behavior

---

## Problem

Every villager is omniscient. `ai_villager()` receives global `stockpile_wood`, `stockpile_stone`, `stockpile_food` counts and uses them to make decisions:

```rust
// ai.rs line 894 — explorer checks GLOBAL wood count
if stockpile_wood < 10 {
    if let Some((fx, fy)) = wood_nearby { ... }
}

// ai.rs line 1152 — stone emergency uses GLOBAL stone count
if stockpile_stone < 5 && !committed_to_build && stone_deposit_visible { ... }

// ai.rs line 1321-1338 — gather priority compares GLOBAL wood vs stone
if stockpile_stone < stockpile_wood / 2 { false }
else if stockpile_wood < stockpile_stone / 2 { true }
```

A villager on the far side of the map knows exactly how much wood is in the stockpile. A newly spawned villager knows every resource location. Nobody needs to explore because everyone already knows the state of the world. This kills Pillar 2 and flattens the Explore phase of Pillar 3.

## Design

### Core Concept

Each villager carries a personal `VillagerMemory` — a bounded collection of things they have personally seen or been told. AI decisions read from memory instead of global state. Memory entries have timestamps and decay, so old information becomes unreliable.

### Memory Entry

```rust
/// A single thing a villager remembers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub kind: MemoryKind,
    pub x: f64,
    pub y: f64,
    pub tick_observed: u64,    // game tick when this was seen/learned
    pub confidence: f32,       // 1.0 = just saw it, decays toward 0.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MemoryKind {
    /// "I know where a stockpile is" — location of a stockpile building
    StockpileLocation,
    /// "I saw trees here" — forest tile suitable for wood gathering
    WoodSource,
    /// "I saw stone here" — stone deposit or mountain edge
    StoneSource,
    /// "I saw food here" — berry bush or farm with pending harvest
    FoodSource,
    /// "I saw a build site here" — incomplete building needing work
    BuildSite,
    /// "I saw wolves here" — predator sighting, triggers avoidance
    DangerZone,
    /// "My home is here" — assigned hut location (does not decay)
    HomeLocation,
    /// "I visited this area" — exploration breadcrumb
    VisitedArea,
}
```

### VillagerMemory Component

```rust
/// Per-villager knowledge store. Replaces global stockpile count reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VillagerMemory {
    /// Fixed-capacity ring buffer of memories. Oldest entries evicted first
    /// when capacity is reached, UNLESS the entry is pinned (HomeLocation,
    /// StockpileLocation).
    entries: Vec<MemoryEntry>,  // max capacity: MEMORY_CAPACITY

    /// Cached "what I think the stockpile has" — updated when visiting stockpile.
    /// None = "I have no idea." Some(counts) = "last time I checked, there was..."
    pub believed_stockpile: Option<BelievedStockpile>,
}

/// What a villager believes the stockpile contains.
/// Updated ONLY when the villager is physically at a stockpile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BelievedStockpile {
    pub food: u32,
    pub wood: u32,
    pub stone: u32,
    pub tick_observed: u64,
}
```

### Constants

```rust
const MEMORY_CAPACITY: usize = 32;        // max entries per villager
const MEMORY_DECAY_RATE: f32 = 0.002;     // confidence lost per tick (~500 ticks to half-life)
const MEMORY_STALE_THRESHOLD: f32 = 0.3;  // below this, memory is "unreliable"
const MEMORY_FORGET_THRESHOLD: f32 = 0.05; // below this, memory is evicted on next cleanup
const STOCKPILE_BELIEF_HALFLIFE: u64 = 300; // ticks before stockpile belief confidence halves
```

The capacity of 32 is chosen to be small enough that villagers genuinely forget things (especially VisitedArea breadcrumbs and old resource sightings), but large enough that a villager who has been around a while has useful knowledge. At scale (500+ villagers), 32 entries * ~40 bytes each = ~1.3 KB per villager = ~640 KB for 500 villagers. Negligible.

### Memory Lifecycle

**1. Observation (Layer 1 -> Layer 2)**

Every tick, during the AI pass, a villager observes what is within `sight_range`. New observations create or refresh memory entries:

```
- Standing near forest tile?       -> upsert WoodSource at (x, y), confidence = 1.0
- Standing near stone deposit?     -> upsert StoneSource at (x, y), confidence = 1.0
- Can see a predator?              -> upsert DangerZone at predator (x, y), confidence = 1.0
- Arrived at stockpile?            -> update believed_stockpile with actual counts
- Arrived at build site?           -> upsert BuildSite at (x, y), confidence = 1.0
- Standing on new terrain?         -> upsert VisitedArea at (x, y), confidence = 1.0
```

"Upsert" means: if an entry of the same kind exists within 5 tiles of (x, y), refresh its confidence and tick. Otherwise, insert a new entry. This prevents 20 WoodSource entries for the same forest.

**2. Decay (every tick)**

Each tick, all entries lose `MEMORY_DECAY_RATE` confidence. Exception: `HomeLocation` and `StockpileLocation` do not decay (pinned).

When confidence drops below `MEMORY_FORGET_THRESHOLD`, the entry is eligible for eviction. Eviction happens lazily when capacity is needed.

`believed_stockpile` decays separately: its effective confidence is `1.0 / (1.0 + (current_tick - tick_observed) / STOCKPILE_BELIEF_HALFLIFE)`. This means a villager who visited the stockpile 300 ticks ago believes half of what they saw might still be true. After 1000+ ticks, they effectively know nothing about stockpile state.

**3. Eviction (when inserting at capacity)**

When `entries.len() == MEMORY_CAPACITY` and a new entry must be inserted:

1. Remove entries below `MEMORY_FORGET_THRESHOLD` confidence.
2. If still full, remove the lowest-confidence non-pinned entry.
3. If still full (all pinned, unlikely), overwrite the oldest VisitedArea.

**4. Querying (during AI decision-making)**

The AI reads memory instead of global state. Key query patterns:

```rust
impl VillagerMemory {
    /// Best-known location for a resource type, weighted by confidence and distance.
    fn best_resource(&self, kind: MemoryKind, from_x: f64, from_y: f64) -> Option<(f64, f64, f32)>;

    /// "Do I think the stockpile needs wood?" — based on believed_stockpile.
    fn believes_stockpile_needs(&self, resource: ResourceType, threshold: u32) -> bool;

    /// "Is there danger near this location?" — checks DangerZone entries.
    fn danger_near(&self, x: f64, y: f64, radius: f64) -> bool;

    /// "Have I explored this area?" — checks VisitedArea entries.
    fn has_visited_near(&self, x: f64, y: f64, radius: f64) -> bool;
}
```

`best_resource` returns the highest-confidence entry of the given kind, with a distance penalty so nearby memories are preferred over distant high-confidence ones:

```
score = confidence - (distance / 100.0)
```

### How AI Decisions Change

The `ai_villager()` function signature changes. Instead of receiving `stockpile_wood: u32, stockpile_stone: u32, stockpile_food: u32`, it receives `&VillagerMemory` (or a summary struct derived from it). Here are the specific decision points that change:

#### 1. "Should I gather wood or stone?" (currently: compare global stockpile counts)

**Before:**
```rust
if stockpile_stone < stockpile_wood / 2 { gather_stone }
```

**After:**
```rust
let needs_wood = memory.believes_stockpile_needs(Wood, 10);
let needs_stone = memory.believes_stockpile_needs(Stone, 10);
let know_wood = memory.best_resource(WoodSource, pos.x, pos.y);
let know_stone = memory.best_resource(StoneSource, pos.x, pos.y);

match (needs_wood, needs_stone, know_wood, know_stone) {
    // Know where both are, stockpile needs both: go to closer one
    (true, true, Some(w), Some(s)) => if w.score > s.score { gather_wood } else { gather_stone },
    // Know where wood is, need wood: gather wood
    (true, _, Some(_), _) => gather_wood,
    // Know where stone is, need stone: gather stone
    (_, true, _, Some(_)) => gather_stone,
    // Don't know where anything is: EXPLORE
    _ => explore,
}
```

The critical difference: a villager who has never seen stone cannot decide to mine stone. They must explore or learn from another villager (Layer 3, future work).

#### 2. "Should I explore?" (currently: `stockpile_wood < 10 || stockpile_stone < 10`)

**Before:** Explore when global stockpile is low.

**After:** Explore when: (a) memory has no resource entries of a needed type, OR (b) all remembered resource locations have low confidence (stale). This means newly spawned villagers explore by default because they know nothing. Experienced villagers explore when their known sources are depleted (they went to a forest and it was stumps).

#### 3. "Stone emergency" (currently: `stockpile_stone < 5`)

**Before:** All villagers globally know stone is critical.

**After:** A villager believes stone is critical only if they visited the stockpile recently (`believed_stockpile.stone < 5` AND confidence is above stale threshold). A villager who hasn't been to the stockpile in 500 ticks has no opinion about stone levels and will continue their current task.

#### 4. "Should I eat from stockpile?" (currently: `has_stockpile_food`)

**Before:** Villager knows globally whether the stockpile has food.

**After:** Villager believes stockpile has food if `believed_stockpile.food > 0` and the belief isn't ancient. A villager with no stockpile memory who is hungry will seek berry bushes within sight range, or wander toward home hoping to find food. This creates visible "lost and hungry" behavior that the player can solve by building closer stockpiles.

#### 5. "Where is the nearest stockpile?" (currently: passed as `&[(f64, f64)]`)

Stockpile locations are pinned memories (HomeLocation, StockpileLocation). A new villager spawned at the settlement starts with StockpileLocation and HomeLocation pre-seeded. But a villager who wanders far and finds a new outpost stockpile will remember both.

#### 6. Farming/Working lease interruption (currently: `stockpile_wood < 5 && stockpile_stone < 5`)

**Before:** A farmer abandons their farm when global wood AND stone are both critically low.

**After:** A farmer only abandons the farm if their believed stockpile (from last visit) shows critical shortage. A farmer who deposited food 50 ticks ago and saw wood = 3 might leave. A farmer who hasn't visited the stockpile in 1000 ticks will keep farming because they have no reason to think there is a crisis.

### Spawning and Initial Memory

When a villager is spawned (birth or migration):

```rust
fn initial_memory(home_x: f64, home_y: f64, stockpile_pos: (f64, f64), tick: u64) -> VillagerMemory {
    let mut mem = VillagerMemory::new();
    mem.insert(MemoryEntry {
        kind: MemoryKind::HomeLocation,
        x: home_x, y: home_y,
        tick_observed: tick,
        confidence: 1.0,  // pinned, won't decay
    });
    mem.insert(MemoryEntry {
        kind: MemoryKind::StockpileLocation,
        x: stockpile_pos.0, y: stockpile_pos.1,
        tick_observed: tick,
        confidence: 1.0,  // pinned, won't decay
    });
    // No believed_stockpile — they haven't visited yet.
    // No resource locations — they must discover or be told.
    mem
}
```

This means new villagers immediately know where home and the stockpile are (they were born there), but know nothing about resource locations. Their first action will be to wander/explore near the settlement, observe terrain, and build up personal knowledge.

### Observation System

A new ECS system runs once per tick, before the AI pass:

```rust
fn system_update_memories(
    world: &mut hecs::World,
    map: &TileMap,
    tick: u64,
    // entity lists for things that can be observed
    stone_deposit_positions: &[(f64, f64)],
    food_source_positions: &[(f64, f64)],
    build_site_positions: &[(f64, f64)],
    predator_positions: &[(f64, f64)],
) {
    for (_, (pos, creature, memory)) in world
        .query_mut::<(&Position, &Creature, &mut VillagerMemory)>()
    {
        if creature.species != Species::Villager { continue; }

        let sr = creature.sight_range;

        // Observe terrain within sight range (sample, don't scan every tile)
        // Sample 8 directions at distances 3, 6, 12 tiles
        for &sample_dist in &[3.0, 6.0, 12.0] {
            for &(dx, dy) in &EIGHT_DIRS {
                let sx = pos.x + dx * sample_dist;
                let sy = pos.y + dy * sample_dist;
                if dist(pos.x, pos.y, sx, sy) > sr { continue; }
                match map.get(sx.round() as usize, sy.round() as usize) {
                    Some(Terrain::Forest) => memory.upsert(WoodSource, sx, sy, tick),
                    Some(Terrain::Mountain) => memory.upsert(StoneSource, sx, sy, tick),
                    _ => {}
                }
            }
        }

        // Observe entities within sight range
        for &(ex, ey) in stone_deposit_positions {
            if dist(pos.x, pos.y, ex, ey) < sr {
                memory.upsert(MemoryKind::StoneSource, ex, ey, tick);
            }
        }
        for &(fx, fy) in food_source_positions {
            if dist(pos.x, pos.y, fx, fy) < sr {
                memory.upsert(MemoryKind::FoodSource, fx, fy, tick);
            }
        }
        for &(bx, by) in build_site_positions {
            if dist(pos.x, pos.y, bx, by) < sr {
                memory.upsert(MemoryKind::BuildSite, bx, by, tick);
            }
        }
        for &(px, py) in predator_positions {
            if dist(pos.x, pos.y, px, py) < sr {
                memory.upsert(MemoryKind::DangerZone, px, py, tick);
            }
        }

        // Decay all entries
        memory.decay_tick();
    }
}
```

**Performance note:** The observation scan samples 24 terrain tiles per villager (8 directions * 3 distances), not the full sight-range circle (~1500 tiles for range 22). Entity observation is O(villagers * entity_list_length), which will need spatial partitioning at scale (Pillar 5) but is fine for 30-100 villagers.

### Stockpile Visit: Updating Beliefs

When a villager arrives at a stockpile (already detected in AI — the Hauling state deposits resources, the Eating state consumes from stockpile), update their belief:

```rust
// In the Hauling -> deposit path and Eating -> stockpile path:
if at_stockpile {
    memory.believed_stockpile = Some(BelievedStockpile {
        food: actual_stockpile.food + actual_stockpile.grain + actual_stockpile.bread,
        wood: actual_stockpile.wood,
        stone: actual_stockpile.stone,
        tick_observed: current_tick,
    });
}
```

This is the critical bridge: the stockpile is the information hub. Villagers who visit frequently have accurate beliefs. Villagers who are far away have stale beliefs or none at all.

### Danger Avoidance

DangerZone memories create soft avoidance. When a villager is choosing a gather target and has multiple options:

```rust
// Penalize targets near remembered danger zones
let danger_penalty = if memory.danger_near(tx, ty, 10.0) { 0.3 } else { 0.0 };
let score = confidence - (distance / 100.0) - danger_penalty;
```

This means villagers will prefer to gather wood from the safe forest to the west rather than the forest to the east where they saw wolves. But if the safe forest is depleted, they will eventually go east (the danger memory decays, or the score of the dangerous option exceeds alternatives).

### Stale Memory: Emergent Misinformation

The most interesting emergent behavior: a villager remembers "there is forest at (50, 20)" with confidence 0.6. They walk there. The forest was cut down by other villagers 200 ticks ago. They arrive, see stumps/grass, and their WoodSource memory for that location is either removed (they can see it is wrong) or replaced with a VisitedArea entry.

This wasted trip is visible and intentional. The player sees a villager walk across the map and come back empty-handed. The solution (which the simulation provides organically): build a stockpile/outpost closer to resources so information flows faster, or build roads so villagers travel faster and their memories stay fresher.

## Migration Path

This feature can be implemented incrementally without breaking existing behavior.

### Step 1: Add VillagerMemory component, no behavior change

- Add `VillagerMemory` to `components.rs`
- Attach it to all villager entities at spawn
- Run `system_update_memories` each tick to populate memories
- AI still reads global stockpile counts (no behavior change)
- Add debug overlay showing a selected villager's memory entries on the map
- **Test:** memories populate correctly, decay works, capacity limits hold

### Step 2: Replace resource-finding with memory queries

- Change `ai_villager`'s gather-target selection to use `memory.best_resource()` instead of `find_nearest_terrain()` and the `stone_deposits` parameter
- Keep global stockpile counts for urgency thresholds (hybrid mode)
- **Test:** villagers still gather effectively. New villagers explore more (they don't know where things are). Experienced villagers are more efficient (they go straight to known sources).

### Step 3: Replace stockpile count reads with beliefs

- Replace `stockpile_wood`, `stockpile_stone`, `stockpile_food` parameters with `memory.believed_stockpile`
- Villagers who haven't visited the stockpile recently default to "I don't know" behavior (continue current task, or wander toward stockpile to check)
- **Test:** settlement still functions. Villagers near the stockpile respond to shortages quickly. Distant villagers respond with a delay proportional to their last visit time.

### Step 4: Danger avoidance

- DangerZone memories influence target selection
- Villagers prefer resource locations away from remembered predator sightings
- **Test:** after a wolf attack from the east, villagers visibly prefer western resources for a while.

## Edge Cases

**New game start:** All villagers spawn together near the stockpile. They all have the same initial memories. Differentiation happens as they spread out and observe different terrain.

**Villager death:** Memory dies with the villager. If the only villager who knew about a distant stone deposit dies, that knowledge is lost. This is intentional and creates value for keeping villagers alive.

**Multiple stockpiles:** Each stockpile is a separate `StockpileLocation` memory. `believed_stockpile` refers to the last stockpile visited (the one this villager uses most). Future: per-stockpile beliefs, but that is unnecessary complexity for now.

**Save/load:** `VillagerMemory` derives `Serialize, Deserialize`. The full memory state round-trips through JSON save files. Memory entries include tick timestamps, which remain valid because the game tick is also saved.

**Migration event:** Migrants arrive with empty memory (only HomeLocation and StockpileLocation). They are initially inefficient, wandering and exploring. This is emergent and correct: newcomers don't know the land.

## Performance Budget

| Component | Per villager | 30 villagers | 500 villagers |
|-----------|-------------|-------------|---------------|
| VillagerMemory storage | ~1.3 KB | ~39 KB | ~640 KB |
| Observation scan (24 samples + entity checks) | ~2 us | ~60 us | ~1 ms |
| Decay tick (32 entries) | ~0.1 us | ~3 us | ~50 us |
| Memory queries during AI | ~0.5 us | ~15 us | ~250 us |
| **Total per tick** | | **~78 us** | **~1.3 ms** |

Well within Pillar 5's budget of 2ms AI at 100 pop, 5ms at 500 pop. The observation scan is the bottleneck and will benefit from the spatial hash grid when that lands.

## Observable Behavior Changes (Pillar 4)

What the player should notice after this system is implemented:

1. **New villagers wander more.** They are discovering the world, not beelining to the optimal resource.
2. **Experienced villagers are efficient.** A villager who has been gathering wood for 1000 ticks knows exactly where the forests are.
3. **Wasted trips happen.** A villager walks to a depleted forest, pauses, then redirects. This is the simulation telling a story.
4. **Stockpile visits matter.** Villagers who just deposited resources immediately react to shortages. Farmers who have been in the fields for 500 ticks are oblivious to a wood crisis.
5. **Death has consequences.** Losing your most-explored villager means the settlement "forgets" distant resources until someone else discovers them.
6. **Danger creates zones.** After a wolf attack, villagers visibly avoid that direction for a while, then gradually return as memory fades.

## Future Extensions (Layer 3 prep)

This design intentionally stops at Layer 2 (personal memory). Layer 3 (shared settlement knowledge) builds on this:

- **Stockpile as bulletin board:** When a villager visits the stockpile, they could also read recent reports from other villagers. Data structure: a `Vec<MemoryEntry>` on the Stockpile entity, written by depositing villagers, read by visiting villagers.
- **Encounter sharing:** Two villagers within 3 tiles could exchange their highest-confidence memories. This creates information flow along traffic paths.
- **Environmental traces:** Worn paths (already tracked in traffic map) could serve as implicit memory — a villager seeing a worn path infers "something useful is in that direction."

These are not part of this design but the `MemoryEntry` data structure is intentionally compatible with all of them.
