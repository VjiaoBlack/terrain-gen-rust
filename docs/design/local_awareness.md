# Sight-Range-Only Awareness

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 2 (Emergent Complexity from Simple Agents)*
*Last updated: 2026-04-01*

## Problem

Every villager is omniscient about the settlement. `ai_villager` receives global stockpile counts (`stockpile_wood`, `stockpile_stone`, `stockpile_food`, `has_stockpile_food`), the full list of all build sites on the map (`build_site_positions`), all stone deposits (`stone_deposit_positions`), all hut positions (`hut_positions`), and all food source positions (`food_positions`) -- regardless of distance. A villager standing at the far edge of the map knows the exact wood count at the stockpile and can see a build site 200 tiles away.

This kills emergence. When every agent has perfect information, they all make the same decision at the same time. There is no inefficiency to observe, no information lag to create drama, no reason for exploration to matter. The settlement behaves like a single mind, not an ant colony.

### Specific global checks in `ai_villager` today

| Check | Location (ai.rs line) | What it reads | Problem |
|-------|----------------------|---------------|---------|
| `has_stockpile_food` | L469, L1099 | Global bool: any food/grain/bread in stockpile | Villager knows stockpile contents from anywhere on map |
| `stockpile_food` count | L469, L1192 | Global u32 from `Resources` | Used for food urgency thresholds (`< 5`, `< 10`) |
| `stockpile_wood` count | L472, L742, L894, L1299-1323 | Global u32 from `Resources` | Drives gather-vs-build priority, wood scarcity search range |
| `stockpile_stone` count | L473, L912, L1152, L1323 | Global u32 from `Resources` | Drives stone emergency mining, gather priority |
| `build_site_positions` (all) | L467, L1196-1221 | Every build site on map | Villager seeks build sites beyond sight range (1.5x sight) |
| `stone_deposit_positions` (all) | L468, L888-892, L1148-1157, L1308-1312 | Every stone deposit on map | Filtered by sight range in some places, but the full list is passed in |
| `food_positions` (all) | L464, L1064-1068, L1257-1261 | Every berry bush on map | Filtered by sight range for eating, but full list available |
| `hut_positions` (all) | L475, L1007-1010 | Every hut on map | Villager finds nearest hut globally at night |
| `stockpile_positions` (all) | L466, L538-542, L585-589, L1100-1103 | Every stockpile on map | Used for flee target, haul target, eat-from-stockpile |
| `frontier` (all) | L477, L1398-1400 | All frontier tiles | Villager picks random frontier tile from global list |

### Specific global checks in `system_ai` (systems.rs)

The snapshot phase (L99-159) collects ALL positions globally into flat vecs that are then handed to every creature's AI function:
- `food_positions`: all `FoodSource` entities
- `prey_positions`: all prey
- `villager_positions`: all villagers
- `predator_positions`: all predators
- `stockpile_positions`: all stockpiles
- `build_site_positions`: all build sites
- `stone_deposit_positions`: all stone deposits
- `hut_positions`: all huts

### Specific global checks in `system_assign_workers` (systems.rs)

`system_assign_workers` (L496+) reads global `Resources` to decide whether workshops have input (`resources.wood >= 12`, `resources.stone >= 2`, `resources.food > 15`). It also counts total villagers globally to reserve a gathering fraction. This system is a central planner -- the opposite of local reasoning.

## Goal

Villagers react only to what they can see within their `sight_range` (currently ~22 tiles). "I see food nearby" not "the stockpile has 50 food." "I see a predator" not "wolves exist on the map." "I see an empty wood pile" not "stockpile_wood < 10."

This is Layer 1 of the Agent Knowledge Architecture from game_design.md (Pillar 2): **what I can see right now.**

## Design

### Principle: replace numbers with percepts

A villager should never ask "what is the global stockpile count?" Instead it asks:
- "Can I see a stockpile from here? Is it visibly full or empty?"
- "Can I see a resource from here?"
- "Can I see a build site from here?"
- "Can I see a predator from here?"

Stockpile fullness becomes a visual signal, not a number. A villager near an empty stockpile acts differently than one near a full stockpile -- and a villager who can't see any stockpile has no information at all.

### What changes

#### A. `system_ai` snapshot phase: filter by sight range

Instead of collecting all positions into flat vecs, the snapshot phase should build a **spatial index** (flat grid or hash map, cell size ~22 to match sight range). Each creature's AI then queries only its local neighborhood.

Concretely, the per-creature call changes from:

```rust
// BEFORE: pass all positions, let ai_villager filter sometimes
let (s, vx, vy, h, dep, claim_site) = ai_villager(
    &pos, &creature, &behavior_state, speed,
    predator_nearby,           // already local (good)
    &food_positions,           // GLOBAL
    &stockpile_positions,      // GLOBAL
    &build_site_positions,     // GLOBAL
    &stone_deposit_positions,  // GLOBAL
    has_food,                  // GLOBAL stockpile count
    stockpile_food_count,      // GLOBAL
    stockpile_wood,            // GLOBAL
    stockpile_stone,           // GLOBAL
    map, skill_mults, rng,
    &hut_positions,            // GLOBAL
    is_night,
    &frontier,                 // GLOBAL
);
```

to:

```rust
// AFTER: pass only what this villager can see
let nearby = spatial_index.query_radius(pos.x, pos.y, creature.sight_range);
let (s, vx, vy, h, dep, claim_site) = ai_villager(
    &pos, &creature, &behavior_state, speed,
    &nearby,                   // contains only visible entities
    map, skill_mults, rng,
    is_night,
);
```

#### B. `ai_villager` signature: replace globals with local percepts

New signature concept:

```rust
pub(super) fn ai_villager(
    pos: &Position,
    creature: &Creature,
    state: &BehaviorState,
    speed: f64,
    nearby: &NearbyEntities,   // everything within sight_range
    map: &TileMap,
    skill_mults: &SkillMults,
    rng: &mut impl rand::RngExt,
    is_night: bool,
) -> (BehaviorState, f64, f64, f64, Option<ResourceType>, Option<Entity>)
```

Where `NearbyEntities` is:

```rust
pub(super) struct NearbyEntities {
    pub predators: Vec<(f64, f64)>,
    pub food_sources: Vec<(f64, f64)>,
    pub stockpiles: Vec<(f64, f64, StockpileState)>,  // position + visual fullness
    pub build_sites: Vec<(Entity, f64, f64, bool)>,     // entity, pos, assigned
    pub stone_deposits: Vec<(f64, f64)>,
    pub huts: Vec<(f64, f64)>,
    pub frontier_tiles: Vec<(f64, f64)>,                // only frontier within sight
}
```

#### C. Stockpile fullness replaces global counts

New component on Stockpile entities:

```rust
/// Visual fullness state, updated each tick from actual Resources.
/// Villagers read this when they can SEE the stockpile, not globally.
#[derive(Debug, Clone, Copy)]
pub enum StockpileFullness {
    Empty,      // 0 of this resource
    Low,        // < 5
    Medium,     // 5-20
    High,       // > 20
}

pub struct StockpileState {
    pub food: StockpileFullness,
    pub wood: StockpileFullness,
    pub stone: StockpileFullness,
}
```

A villager that can see a stockpile reads its `StockpileState`. A villager that cannot see any stockpile has NO information about resource levels -- it must decide based on what it can personally see (nearby trees, nearby stone, nearby food).

#### D. Decision table: global check -> local replacement

| Current global check | New local check | Behavior change |
|---------------------|-----------------|-----------------|
| `has_stockpile_food` (bool) | Can I see a stockpile? Is its food state != Empty? | Villager far from stockpile doesn't know food exists; seeks berry bushes instead |
| `stockpile_food < 5` (food urgency) | Visible stockpile has food == Low or Empty | Only villagers near stockpile feel urgency. Others keep doing what they were doing |
| `stockpile_wood < 10` (gather priority) | Visible stockpile has wood == Low or Empty | Villagers far from stockpile gather whatever they find, not what the settlement "needs" |
| `stockpile_stone < 5` (stone emergency) | Visible stockpile has stone == Low or Empty | Stone emergency only triggers for villagers who can see the stockpile |
| `stockpile_wood < 5 && stockpile_stone < 5` (farming break-off) | Visible stockpile has both wood == Low/Empty and stone == Low/Empty | Farmers only abandon farm if they see the shortfall |
| `build_sites` with 1.5x sight range | `build_sites` within actual sight range only | Villagers only build what they can see. Distant build sites need someone to walk past them |
| `stone_deposit_positions` (full list) | Deposits within sight range only | Already partially filtered, but now strictly enforced at query level |
| `hut_positions` (full list) | Huts within sight range | Villager at night seeks only visible huts. Can't see a hut? Sleeps outdoors |
| `frontier` (full list) | Frontier tiles within sight range, or remembered locations | Villagers explore toward what they can see at the edge of the known world, not teleport-targeting random frontier tiles |
| `food_positions` (full list) | Food sources within sight range | Already mostly filtered, now strictly enforced |

#### E. `system_assign_workers` becomes local

The central worker assignment system (`system_assign_workers`) currently acts as a global planner. Under local awareness, it transforms:

**Before:** "There are 3 idle villagers and 2 farms that need workers. Assign villager A to farm 1."

**After:** Each villager, when idle and near a farm/workshop, self-assigns. The decision lives inside `ai_villager`:
- "I'm idle. I can see a farm nearby with no worker. I'll go tend it."
- "I'm idle. I can see a workshop nearby with wood piled outside. I'll go work."

The villager checks what buildings are visible and whether they look like they need help (no visible worker nearby, input resources visible). This replaces the centralized `system_assign_workers` with distributed, local decisions.

**Migration note:** `system_assign_workers` can initially remain as a fallback for villagers within sight range of their target. The key change is that it stops assigning villagers to buildings they can't see.

### What stays global (for now)

Some things legitimately remain global or semi-global:

| Check | Why it stays global | Future local version |
|-------|-------------------|---------------------|
| `wolf_aggression` | Game-level difficulty parameter, not agent knowledge | Could become per-wolf hunger threshold |
| `settlement_defended` | Garrison existence is a settlement-level fact | Wolf checks if it can see a garrison |
| `is_night` | Time of day is globally observable | Stays global forever (everyone sees the sky) |
| `skill_mults` | Civilization-level skill progression | Per-villager skills (Layer 2: memory) |
| `predator_nearby` check | Already local -- uses sight range | Keep as-is |

### Spatial index implementation

Introduce a simple grid-based spatial index:

```rust
pub struct SpatialGrid {
    cell_size: f64,            // ~22.0 to match sight range
    cells: HashMap<(i32, i32), Vec<SpatialEntry>>,
}

pub struct SpatialEntry {
    pub entity: Entity,
    pub x: f64,
    pub y: f64,
    pub kind: EntityKind,      // Predator, Prey, Villager, FoodSource, Stockpile, BuildSite, etc.
}

impl SpatialGrid {
    pub fn query_radius(&self, x: f64, y: f64, radius: f64) -> Vec<&SpatialEntry> {
        // Check all cells that overlap the radius, filter by exact distance
    }
}
```

This is also the foundation for the Pillar 5 performance optimization (spatial hash grid). Building it now for awareness serves double duty.

**Performance:** Currently `system_ai` does O(entities) work per creature to filter positions. The spatial grid makes this O(nearby_entities) per creature. Net effect is neutral or positive for 30 villagers, and dramatically better at 500+.

## Migration path

### Phase 0: Spatial index infrastructure (no behavior change)

1. Add `SpatialGrid` struct to `ecs/` (new file `spatial.rs` or in `systems.rs`).
2. In `system_ai`, build the spatial grid from the existing snapshot vecs at the start of Phase 1.
3. Replace the flat vec lookups with spatial grid queries, but pass the same sight range that's currently used. All existing tests pass with identical behavior.

**Validation:** Run full test suite. Behavior is byte-identical because the spatial grid returns the same results as the current distance filters -- it just does it more efficiently.

### Phase 1: Strict sight-range filtering (behavior change: minor)

1. Remove the 1.5x sight range multiplier on build site searches. Villagers see build sites within their actual `sight_range` only.
2. Remove the 1.5x wood search range for critically low wood. Villagers search within actual `sight_range`.
3. Filter `hut_positions` by sight range (currently global).
4. Filter `frontier` tiles by sight range (currently global random selection).
5. Filter `stockpile_positions` by sight range for eat-from-stockpile decisions.

**Behavior impact:** Villagers are slightly less efficient. Build sites at the edge of auto-build radius take longer to discover. Villagers without visible huts sleep outdoors. This is intentional -- it's the start of local reasoning.

**Validation:** Existing integration tests may need threshold adjustments (e.g., settlement takes longer to build N structures). Add new tests: "villager at distance > sight_range from build site does NOT seek it."

### Phase 2: Stockpile fullness replaces global counts (behavior change: moderate)

1. Add `StockpileState` component to Stockpile entities. Update it each tick from global `Resources`.
2. Replace `stockpile_wood`, `stockpile_stone`, `stockpile_food`, `has_stockpile_food` parameters with a `Option<StockpileState>` that is `Some` only if a stockpile is within sight range.
3. When `visible_stockpile` is `None`, the villager has no resource urgency information. It gathers whatever it finds nearby (wood if near forest, stone if near mountain, food if near bushes) or wanders/explores.
4. When `visible_stockpile` is `Some(state)`, the villager reads the fullness tiers to prioritize: `state.wood == Low` triggers wood gathering, etc.

**Behavior impact:** This is the big one. Villagers far from the stockpile become genuinely independent agents. They gather what's local, haul it back, and only learn the settlement's needs when they arrive at the stockpile. You'll see:
- Villagers near the stockpile responding quickly to shortages
- Villagers far away continuing to gather whatever is nearby
- Information lag: a wood shortage is only "felt" by villagers who can see the stockpile
- Natural clustering: villagers near the stockpile become the responsive core; distant ones are frontier gatherers

**Validation:** New tests for "villager without visible stockpile gathers nearest resource regardless of global counts." Playtest: watch a 10K-tick settlement and verify it still survives on multiple seeds. Tune `StockpileFullness` thresholds if settlements collapse.

### Phase 3: Local worker self-assignment (behavior change: moderate)

1. Move farm/workshop assignment logic from `system_assign_workers` into the `ai_villager` idle/wander decision branch.
2. Idle villager sees a farm within sight range with no visible worker nearby -> enters `Farming` state.
3. Idle villager sees a workshop within sight range with visible input resources and no visible worker -> enters `Working` state.
4. Remove or reduce `system_assign_workers` to only handle edge cases (e.g., ensuring at least one farmer if all villagers went gathering).

**Behavior impact:** Worker assignment becomes organic. A villager who happens to walk past an unmanned farm will tend it. Farms far from the settlement core may go unattended until someone wanders by. This creates visible "that farm needs a worker" situations that the player can solve by building a hut nearby (attracting villagers to the area).

**Validation:** Track farming_ticks and food production across 10K-tick runs. If food production drops too much, the self-assignment idle check may need to trigger at slightly longer range or with a small probability of "remembering" a farm location (preview of Layer 2: memory).

### Phase 4: Remove remaining global params from ai_villager

1. Remove `stockpile_wood`, `stockpile_stone`, `stockpile_food`, `has_stockpile_food` from the function signature entirely.
2. Remove `build_site_positions` as a separate param -- it's now part of `NearbyEntities`.
3. The only non-local params remaining are: `is_night`, `skill_mults` (both legitimately global).

## Risks and mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Settlements starve because villagers don't respond to food shortages fast enough | High | Tune `StockpileFullness::Low` threshold generously. Stockpile visual signal propagates instantly to nearby villagers. Early game stockpile is always central, so most villagers see it. |
| Build sites never get built because no villager walks past them | Medium | Auto-build already places buildings near the settlement center. For edge cases, the exploration behavior naturally sends villagers outward. If needed, add a "I see unbuilt foundation" attractor at slightly longer range for builders specifically. |
| Performance regression from spatial index overhead at 30 villagers | Low | Spatial grid overhead is minimal (hash lookups). Profile Phase 0 to confirm. If slower, keep flat vecs for < 100 entities and switch to grid above that. |
| Too many behavior changes at once makes debugging impossible | High | Each phase is independently deployable and testable. Phase 0 changes zero behavior. Phase 1 is minor. Only Phase 2+ changes observable behavior significantly. Never merge two phases in one commit. |
| `system_assign_workers` removal causes workshop/farm starvation | Medium | Keep `system_assign_workers` as fallback during Phase 3, gated behind a proximity check. Only fully remove it after playtesting confirms self-assignment works. |

## Testing strategy

### Unit tests (per phase)

- **Phase 0:** Spatial grid returns same entities as brute-force distance filter for known configurations.
- **Phase 1:** Villager at distance > `sight_range` from build site returns Wander/Idle, not Seek{BuildSite}. Villager at distance > `sight_range` from hut sleeps outdoors at night.
- **Phase 2:** Villager with `visible_stockpile: None` gathers nearest resource type, ignoring global counts. Villager with `visible_stockpile: Some(StockpileState { wood: Low, .. })` prioritizes wood.
- **Phase 3:** Idle villager within sight of unmanned farm enters Farming state. Idle villager beyond sight of any farm does not enter Farming.

### Integration tests

- 10K-tick settlement survival on seeds 42 and 137 after each phase. Settlement must reach pop > 10 and survive first winter.
- Behavior distribution audit: log what fraction of villagers are in each BehaviorState per tick. Verify the distribution still shows gathering, farming, building, sleeping in reasonable proportions.

### Playtest criteria

- Watch a settlement for 5K ticks. Can you see villagers making *different* decisions based on where they are? (Villager near forest gathers wood; villager near stockpile responds to shortages.)
- Does the settlement feel like an ant colony (distributed, slightly chaotic, locally smart) rather than a hivemind (everyone does the same thing at once)?

## Future: Layers 2-3

This doc covers Layer 1 only (what I can see right now). The architecture is designed to support:

- **Layer 2 (memory):** Villager remembers where it last saw a forest, stockpile, danger. Memory struct per villager with timestamps. Stale memory motivates re-scouting. `ai_villager` checks memory when nothing is visible.
- **Layer 3 (shared knowledge):** Villager arriving at stockpile "uploads" knowledge (resource locations, danger zones). Other villagers "download" when they visit. Stockpile becomes a bulletin board. Information spreads at the speed of villager travel, not instantly.

These layers build on sight-range-only awareness. Without Layer 1, Layers 2-3 are pointless (why remember things if you already know everything?).
