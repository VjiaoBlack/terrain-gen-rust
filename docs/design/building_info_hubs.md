# Design: Building-as-Hub (Specialized Information Hubs)

**Status:** Proposed
**Pillars:** 2 (Emergent Complexity from Simple Agents), 3 (Explore -> Expand -> Exploit -> Endure), 4 (Observable Simulation)
**Phase:** Economy Depth (Phase 2)
**Depends on:** Stockpile bulletin board, per-villager memory
**Unlocks:** Role specialization through knowledge, reason to visit specific buildings, emergent districts, military coordination

---

## Problem

The stockpile bulletin board design (see `stockpile_bulletin_board.md`) establishes the pattern: a building holds a `BulletinBoard`, villagers read/write it on visit, and information spreads at the speed of foot traffic. But the stockpile is the only hub. Every type of information — resource locations, danger sightings, build site needs, material shortages — lives on the same board.

This creates two problems:

1. **No reason to visit other buildings for information.** A villager who visits the stockpile learns everything. The garrison, the workshop, the granary are just production sites. They don't pull villagers toward them the way the stockpile does.

2. **No specialization of knowledge.** A farmer and a soldier read the same board and get the same information. There is no concept of "the garrison knows where the threats are" or "the workshop knows what materials are needed." Every building is informationally identical.

In a real settlement, different places hold different knowledge. The blacksmith knows what tools are needed. The watchtower knows where enemies were last seen. The market knows what goods are available. Visiting a specific building gives you specific, useful information that you cannot get elsewhere.

## Design

### Core Concept

Each building type maintains its own bulletin board with **domain-specific posts**. The stockpile board tracks resource locations. The garrison board tracks threat intelligence. The workshop board tracks material needs and production status. A villager visiting a building reads that building's specialized board and writes relevant observations to it.

This means a villager must visit multiple buildings to be fully informed. A gatherer who only visits the stockpile knows where resources are but not where wolves patrol. A soldier who only visits the garrison knows threat patterns but not where the stone deposits are. Villagers who travel between buildings become knowledge bridges.

### Building Board Definitions

#### Stockpile: Resource Intelligence

The stockpile board is unchanged from `stockpile_bulletin_board.md`. It tracks what the land offers and where.

**Post types:**
- `ResourceSighting { resource: ResourceType }` — "There is wood/stone/food at (x, y)"
- `ResourceDepleted { resource: ResourceType }` — "The resource at (x, y) is gone"
- `FertileLand` — "There is good farmland at (x, y)"

**Who writes:** Any villager depositing resources or returning from exploration.

**Who reads:** Gatherers looking for their next target. Builders checking where materials can be sourced. Explorers checking which directions are already mapped.

**Villager behavior after reading:** Updates personal memory with resource locations. May change gather target to a closer or richer source. May decide to explore an unmapped direction (no posts from that quadrant).

#### Garrison: Threat Intelligence

The garrison board is the settlement's collective memory of danger. Soldiers, returning scouts, and villagers who fled from predators all contribute.

**Post types:**

```rust
pub enum GarrisonPostKind {
    /// "I saw wolves near (x, y), heading (direction)"
    PredatorSighting {
        predator_type: Species,
        heading: Option<(f64, f64)>,  // normalized direction vector, if observed
        count: u8,                     // how many were seen together
    },
    /// "Wolves attacked from (x, y)" — confirmed hostile contact
    AttackReport {
        predator_type: Species,
        severity: AttackSeverity,      // Scout (1-2), Pack (3-5), Surge (6+)
    },
    /// "Area (x, y) has been clear for a while" — safe zone report
    AreaCleared,
    /// "Chokepoint at (x, y) would be good for a garrison/wall"
    DefensiveSuggestion,
}

#[derive(Debug, Clone, Copy)]
pub enum AttackSeverity {
    Scout,   // 1-2 predators
    Pack,    // 3-5 predators
    Surge,   // 6+ predators
}
```

**Who writes:**
- Villagers entering `FleeHome` state write a `PredatorSighting` post when they next visit the garrison (or the stockpile, which forwards danger reports to the nearest garrison — see Cross-Building Forwarding below).
- Garrison defenders who survive a raid write an `AttackReport`.
- Patrol villagers (soldiers returning from garrison-assigned patrol routes) write `AreaCleared` for zones they walked through without seeing threats.

**Who reads:**
- Soldiers picking patrol routes. They prioritize directions with recent `PredatorSighting` posts.
- Gatherers checking whether their intended destination is in a danger zone. A gatherer who reads the garrison board before heading out avoids areas with recent sightings.
- The auto-build system reads the garrison board to decide where to place walls and new garrisons. `DefensiveSuggestion` posts from soldiers who noticed chokepoints inform placement.

**Villager behavior after reading:** Soldiers adjust patrol routes toward recent sighting locations. Gatherers add danger penalties to remembered resource locations that overlap with threat zones. Builders may prioritize wall construction near attack corridors.

**Staleness:** `PredatorSighting` posts expire after 3000 ticks (wolves move). `AttackReport` posts persist for 8000 ticks (attack corridors are more stable). `AreaCleared` posts expire after 2000 ticks.

#### Workshop / Smithy: Material Needs

Workshop and smithy boards track what materials are needed for production and what is currently being processed. This creates pull-based gathering: instead of villagers checking global stockpile counts to decide what to gather, they read the workshop board to learn what the production chain actually needs.

**Post types:**

```rust
pub enum WorkshopPostKind {
    /// "I need 2 wood to make planks" — active material request
    MaterialNeeded {
        resource: ResourceType,
        quantity: u32,
        urgency: f32,          // 0.0 = nice to have, 1.0 = production halted
    },
    /// "I just produced 1 plank" — output available for pickup
    OutputReady {
        resource: ResourceType,
        quantity: u32,
    },
    /// "I have been idle for 500 ticks because nobody brought stone"
    ProductionStalled {
        missing_resource: ResourceType,
        idle_since: u64,
    },
}
```

**Who writes:**
- The workshop/smithy worker writes `MaterialNeeded` when their input buffer is low (fewer than 2 batches of input material on hand).
- The worker writes `OutputReady` when a production cycle completes and output is sitting in the building.
- The worker writes `ProductionStalled` when they have been idle for more than 200 ticks due to missing input.

**Who reads:**
- Gatherers who visit the workshop check the board. If there is a `MaterialNeeded` post for wood, and the gatherer knows where wood is (from the stockpile board or personal memory), they prioritize gathering that resource. This creates directed gathering instead of "whatever is closest."
- Haulers check for `OutputReady` posts. A hauler visiting the workshop sees "1 plank ready" and picks it up for transport to the stockpile. Without this post, haulers only check the stockpile.
- The auto-build system reads `ProductionStalled` posts. A smithy stalled on stone for 1000 ticks might trigger a higher priority for stone gathering across the settlement, or suggest building a stockpile closer to a stone source.

**Villager behavior after reading:** Gatherers update their personal priority list — "the workshop needs wood" overrides the default "stockpile is low on stone" if the gatherer learned both facts. Creates visible behavior where a villager leaves the workshop, walks past the stockpile without stopping, and heads straight for a forest because they know the workshop is waiting.

**Staleness:** `MaterialNeeded` posts refresh every time the worker checks their input buffer (every ~100 ticks while working). Stale `MaterialNeeded` posts (>500 ticks old) are assumed resolved. `OutputReady` posts are removed when a hauler picks up the output. `ProductionStalled` posts persist until production resumes.

#### Granary / Bakery: Food Chain Intelligence

The granary and bakery boards track food supply chain status — how much grain is stored, whether bread production is running, and seasonal preparation state.

**Post types:**

```rust
pub enum GranaryPostKind {
    /// "Grain reserves are at N" — snapshot of stored grain
    GrainLevel { quantity: u32 },
    /// "I need food input for grain processing"
    FoodNeeded { quantity: u32 },
    /// "Winter is coming and grain reserves are below safe threshold"
    WinterWarning { deficit: u32, ticks_until_winter: u64 },
}
```

**Who writes:**
- The granary worker updates `GrainLevel` each time they process food into grain.
- The granary worker writes `FoodNeeded` when input buffer is low.
- A `WinterWarning` is generated automatically when the season system reports autumn AND grain reserves are below `population * 3`.

**Who reads:**
- Farmers read the granary board. If `FoodNeeded` is posted, farmers prioritize harvesting ready crops and delivering food. If `WinterWarning` is up, all farmers shift to maximum food production.
- Gatherers who visit the granary learn about food needs and may prioritize berry bushes or hunting over wood/stone.
- The bakery worker checks the granary's `GrainLevel` post to know if grain supply is flowing.

**Villager behavior after reading:** Farmers who read a `WinterWarning` stop any non-food tasks (they were maybe hauling stone for a build site) and focus entirely on food production. This creates a visible seasonal shift in behavior — autumn arrives, a farmer visits the granary, and immediately changes course toward the fields.

#### Farm: Crop Status

Farms are simpler. Their board is less about posting knowledge and more about broadcasting their state so passing villagers know whether to tend them.

**Post types:**

```rust
pub enum FarmPostKind {
    /// "Crops are at growth stage N/100, ready to harvest at 100"
    CropStatus { growth: u8 },
    /// "Harvest ready, need someone to collect"
    HarvestReady,
    /// "Soil fertility is low, yield will be poor"
    LowFertility { fertility: f32 },
}
```

**Who writes:** The farm "writes" its own status automatically as part of the crop simulation tick (not a villager action — the farm entity updates its board each time growth advances).

**Who reads:** Villagers passing near a farm can read its status. A farmer who sees `HarvestReady` will prioritize that farm. A villager looking for work sees `HarvestReady` on a nearby farm board and decides to harvest even if they are not a dedicated farmer. `LowFertility` posts inform the auto-build system that new farms should be placed elsewhere.

### Data Structures

```rust
/// A building's specialized bulletin board.
#[derive(Debug, Clone)]
pub enum BuildingBoard {
    Stockpile(BulletinBoard),                   // existing, unchanged
    Garrison(Vec<GarrisonPost>),
    Workshop(Vec<WorkshopPost>),
    Granary(Vec<GranaryPost>),
    Farm(Vec<FarmPost>),
}

/// Wrapper for a typed post with common metadata.
#[derive(Debug, Clone)]
pub struct TypedPost<K> {
    pub kind: K,
    pub location: (usize, usize),
    pub posted_tick: u64,
    pub reporter: Option<Entity>,  // None for auto-generated posts (farm status)
}

pub type GarrisonPost = TypedPost<GarrisonPostKind>;
pub type WorkshopPost = TypedPost<WorkshopPostKind>;
pub type GranaryPost  = TypedPost<GranaryPostKind>;
pub type FarmPost     = TypedPost<FarmPostKind>;
```

Each building entity gets a `BuildingBoard` component matching its `BuildingType`. The board is created when the building is constructed.

### Board Capacity and Pruning

| Building Type | Max Posts | Stale Threshold | Pruning Trigger |
|---------------|-----------|-----------------|-----------------|
| Stockpile     | 64        | 5000 ticks      | On write        |
| Garrison      | 32        | 3000-8000 ticks (varies by post type) | Every 500 ticks |
| Workshop      | 16        | 500 ticks       | On write        |
| Granary       | 16        | 1000 ticks      | On write        |
| Farm          | 4         | N/A (overwritten each growth tick) | On write |

Workshop and farm boards are intentionally small. They represent the building's current state, not a historical record. The garrison board is larger because threat history matters over longer timescales.

## Interaction Protocol

### When Does a Villager Read a Board?

A villager reads a building's board when they are **physically at the building** (distance < 1.5 tiles) during certain state transitions:

| Building | Read Trigger | Typical Frequency |
|----------|-------------|-------------------|
| Stockpile | Depositing resources, eating from stockpile | Every ~200-400 ticks |
| Garrison | Soldier reporting for duty, villager fleeing past garrison | Every ~300-600 ticks for soldiers |
| Workshop | Worker starting shift, gatherer delivering materials | Every ~100-300 ticks for workers |
| Granary | Farmer delivering food, worker starting processing | Every ~200-400 ticks |
| Farm | Farmer beginning tend cycle | Every ~100-200 ticks for farmers |

Crucially, a villager does NOT read a board they walk past. They must stop at the building — the visit is intentional, not passive. This preserves the information cost: learning something requires spending travel time.

### What Does a Villager Do With Board Information?

After reading a board, the villager stores relevant entries in their `VillagerMemory` using existing memory infrastructure. New `MemoryKind` variants extend the existing enum:

```rust
// Additions to MemoryKind from per_villager_memory.md
pub enum MemoryKind {
    // ... existing variants ...

    /// "The garrison reported wolves near (x, y)" — learned from garrison board
    ThreatReport,
    /// "The workshop needs wood" — learned from workshop board
    MaterialRequest { resource: ResourceType },
    /// "The granary is low, winter is coming" — learned from granary board
    WinterShortage,
    /// "Farm at (x, y) needs harvesting" — learned from farm board
    HarvestAvailable,
}
```

These new memory kinds participate in the same decay, capacity, and eviction system as existing memories. A villager who learned about a wolf sighting from the garrison 2000 ticks ago will have that memory fade, just like a firsthand sighting.

### Writing Back to Boards

Villagers write to the appropriate board based on what they observed:

| Observation | Written To | Post Kind |
|-------------|-----------|-----------|
| Saw predators while gathering | Garrison (next visit) | `PredatorSighting` |
| Was attacked by wolves | Garrison (next visit) | `AttackReport` |
| Completed patrol of an area | Garrison (on return) | `AreaCleared` |
| Found a natural chokepoint | Garrison (on return) | `DefensiveSuggestion` |
| Gathered resources for a workshop | Workshop (on delivery) | Clears corresponding `MaterialNeeded` |
| Noticed a stalled workshop | Workshop (on visit) | No write; the worker writes `ProductionStalled` |
| Harvested a farm | Farm (automatic) | `CropStatus` updates, `HarvestReady` removed |

The key rule: **villagers write to the board of the building they are visiting, about topics relevant to that building.** A villager does not write threat intel to the stockpile board. They carry the `DangerZone` memory in their head until they visit a garrison.

## Cross-Building Forwarding

Some information is urgent enough that it should flow between buildings without requiring a villager to visit both. A single forwarding rule handles this:

**Danger forwarding:** When a villager writes a `PredatorSighting` to ANY building's board (because they fled to the nearest building, which might be a workshop), the system copies that post to the nearest garrison's board within 20 tiles. Rationale: wolf sightings are life-or-death. Making villagers walk to the garrison before the settlement can react would be punishing, not interesting.

This is the ONLY automatic forwarding. All other information requires physical villager transport. This keeps the system honest — the garrison knows about threats quickly (because that is its purpose), but workshops do not magically learn about resource locations.

## AI Decision Integration

### Gather Priority (modified from per_villager_memory.md)

The existing gather decision from `per_villager_memory.md` step 3 ("Score each known location") gains new inputs:

```
score = confidence
      - (distance / 100.0)
      - danger_penalty           // from garrison board, via ThreatReport memory
      + workshop_bonus            // if workshop needs this resource, +0.3
      + winter_bonus              // if granary posted WinterWarning, food sources get +0.5
```

A villager who has visited both the stockpile and the workshop makes better decisions than one who has only visited the stockpile. The workshop bonus means "the settlement's production chain needs this" which is a stronger signal than "the stockpile is low."

### Soldier Patrol Routes

Soldiers assigned to a garrison currently patrol randomly or stand guard. With the garrison board:

1. Soldier reads garrison board on shift start.
2. If `PredatorSighting` posts exist, soldier patrols toward the most recent sighting direction.
3. If no recent sightings, soldier patrols the direction with the oldest `AreaCleared` post (re-scout stale areas).
4. If no posts at all (new garrison), soldier patrols in expanding concentric arcs.
5. On return, soldier writes `AreaCleared` for zones traversed without contact, or `PredatorSighting`/`AttackReport` if threats were encountered.

This creates visible, intelligent patrol behavior: soldiers concentrate patrols in the direction wolves have been coming from, and gradually expand coverage as threat directions shift.

### Worker Material Requests

Workshop/smithy workers currently consume input materials from their building's stockpile and stall when empty. With the workshop board:

1. Worker notices input buffer < 2 batches. Writes `MaterialNeeded { wood, 2, urgency: 0.5 }`.
2. If input buffer reaches 0, worker writes `ProductionStalled { wood, idle_since: current_tick }`.
3. Urgency escalates: `0.5` at low input, `0.8` when stalled <200 ticks, `1.0` when stalled >200 ticks.
4. A gatherer visits the workshop (maybe delivering an earlier haul). Reads the board. Sees `MaterialNeeded { wood, 2, urgency: 0.8 }`.
5. The gatherer now has a `MaterialRequest { Wood }` memory with confidence proportional to urgency.
6. On their next gather decision, wood sources get a +0.3 bonus. The gatherer prioritizes wood.
7. Gatherer delivers wood to the workshop. The `MaterialNeeded` post is cleared by the worker when they pick up the input.

The visible result: a workshop runs out of wood. A gatherer visits. The gatherer turns around and walks directly to a known forest. Returns with wood. The workshop resumes. The player sees cause and effect without any global priority system.

### Seasonal Behavior Shifts

The granary's `WinterWarning` creates a settlement-wide behavioral shift, but it spreads through physical visits, not a global flag:

1. Autumn begins. Granary worker calculates deficit. Writes `WinterWarning { deficit: 15, ticks_until_winter: 2000 }`.
2. Farmers who visit the granary (to deliver food) read the warning. They gain a `WinterShortage` memory.
3. Farmers with `WinterShortage` memory abandon non-food tasks and focus on harvesting/tending.
4. Gatherers who visit the granary gain the same memory. They switch to foraging for berry bushes.
5. Villagers who NEVER visit the granary (a distant miner, for instance) do not learn about the warning until they visit a building where someone relayed it — which might not happen until they return to the stockpile and meet a farmer who tells them.

The visible result: autumn hits. Over the course of ~500 ticks, a wave of behavioral change ripples outward from the granary. Nearby farmers react immediately. Distant gatherers react when they next visit. The most isolated miners might not react at all — they keep mining while the settlement scrambles for food. The player can see who knows and who does not (via knowledge overlay).

## Observable Behavior (Pillar 4)

What the player should see that they could not see before:

1. **Directed movement after building visits.** A villager exits the garrison and walks northeast — toward a recent wolf sighting. Another exits the workshop and heads for the forest — the workshop needs wood. A third exits the granary and runs to the fields — winter is coming. Each building sends villagers in purposeful, different directions.

2. **Information lag creates visible drama.** Wolves attack from the south. The garrison knows. Soldiers react. But a gatherer heading south from the workshop does not know yet — they have not visited the garrison. The player watches them walk into danger, and understands why: they lacked the information.

3. **Building visits as events.** A farmer arrives at the granary, pauses (reads the board), and immediately changes direction. The "pause and redirect" moment is a visible information transfer. The player learns to read these moments.

4. **Emergent building importance.** The garrison becomes a busy hub after a raid — every villager with a danger memory detours to report. The workshop becomes busy when production chains are active. The granary becomes the center of attention in autumn. Different buildings matter at different times, and the player can see which building is "hot" by watching foot traffic.

5. **Knowledge overlay per building.** The existing knowledge overlay (described in `stockpile_bulletin_board.md`) extends to show building-specific knowledge. Toggle to "garrison knowledge" and see the threat map — areas colored by recency of patrol reports. Toggle to "workshop knowledge" and see material need intensity. Each building's board paints a different picture of the settlement's information state.

## Emergent Consequences

Things that happen without extra code:

1. **Natural role specialization.** A villager who works at the workshop and visits the stockpile regularly has excellent resource knowledge AND production chain awareness. A soldier who patrols and returns to the garrison has excellent threat knowledge. Neither has the other's knowledge. Specialization emerges from which buildings a villager frequents.

2. **Cross-pollination through multi-building visitors.** A hauler who carries planks from the workshop to a build site, then deposits leftover wood at the stockpile, then passes the garrison on the way back — this villager has read three boards and carries a rich, diverse memory. They are informationally valuable. Haulers become knowledge bridges naturally.

3. **Building placement affects information flow.** A garrison placed near the stockpile means every returning gatherer sees the threat board on their way through. A garrison placed far away means threat intel stays isolated until soldiers relay it. The player's building placement decisions shape information topology.

4. **Crisis cascades become visible.** Wolves kill a gatherer. Their knowledge dies (per `per_villager_memory.md`). But the garrison board still has the attack report, so soldiers can patrol toward the attack site. The workshop that the gatherer was supplying writes `ProductionStalled`. A different gatherer visits the workshop, learns about the stall, and takes over. The cascade of reaction flows through the buildings, visible at each step.

5. **Outpost garrisons create early warning.** A garrison built at the settlement's frontier gets the first threat reports from explorers and border gatherers. Soldiers stationed there patrol the frontier. If the player builds a road between the frontier garrison and the main settlement, soldiers rotate faster and threat intel flows faster. Infrastructure serves information, not just logistics.

6. **Seasonal preparation as emergent collective behavior.** No global "prepare for winter" flag. The granary posts a warning. Villagers who visit the granary react. They tell others indirectly by changing the settlement's behavior (more food at the stockpile, less wood gathering). The preparation wave is visible and imperfect — some villagers never get the message, creating natural inefficiency the player can address by building better infrastructure.

## Performance

**Per building board:**
- Garrison: 32 posts * ~48 bytes = ~1.5 KB
- Workshop: 16 posts * ~40 bytes = ~640 bytes
- Granary: 16 posts * ~36 bytes = ~576 bytes
- Farm: 4 posts * ~24 bytes = ~96 bytes

**At scale (500 villagers, ~20 buildings):**
- Total board memory: ~20 buildings * ~1 KB avg = ~20 KB. Negligible.
- Board reads happen only on building visits (state transitions). At 500 villagers, approximately ~5-10 building visits per tick across all villagers. Each visit scans a board of 4-64 entries. Total: O(300) work per tick. Negligible.
- Board pruning: O(board_size) on write, ~2-3 writes per tick. Negligible.
- No per-tick cost for buildings with no visitors.

The system scales with building count and visit frequency, not with villager count. Visit frequency is bounded by travel time, so more villagers does not mean proportionally more board operations.

## Testing Strategy

**Unit tests:**
- `test_garrison_board_predator_sighting`: writing and reading predator sightings roundtrips correctly.
- `test_garrison_board_staleness`: sightings expire at 3000 ticks, attack reports at 8000.
- `test_workshop_board_material_needed_cleared`: delivering materials removes the corresponding `MaterialNeeded` post.
- `test_workshop_board_urgency_escalation`: urgency increases as stall duration grows.
- `test_granary_winter_warning`: warning is posted when grain < population * 3 in autumn.
- `test_farm_board_auto_update`: farm board updates crop status each growth tick without villager action.
- `test_danger_forwarding`: predator sighting written to a workshop board is forwarded to the nearest garrison.
- `test_board_capacity_limits`: boards respect their max post limits and prune correctly.

**Integration tests:**
- `test_soldier_patrols_toward_sighting`: spawn garrison + soldier. Post predator sighting to the east. Assert soldier patrols eastward.
- `test_gatherer_avoids_danger_zone`: spawn gatherer with wood sources east (danger) and west (safe). Gatherer reads garrison board with eastern threat. Assert gatherer goes west.
- `test_workshop_stall_triggers_gathering`: workshop posts `ProductionStalled { wood }`. Gatherer visits workshop. Assert gatherer's next target is a wood source.
- `test_winter_warning_spreads`: granary posts warning. Run 1000 ticks. Assert that farmers who visited the granary have `WinterShortage` memory, and farmers who did not visit do not.
- `test_information_stays_local`: post threat to garrison A. Assert villagers who only visit garrison B (separate, distant) do not learn about the threat.

**Behavioral tests (N-tick simulation):**
- After a wolf raid from the east, >70% of patrol routes should trend eastward within 500 ticks.
- After a workshop stalls on wood, wood gathering rate should increase within 800 ticks (time for a gatherer to visit the workshop and return with wood).
- In autumn, food gathering should increase by >50% within 1000 ticks of the first `WinterWarning` post.

## Non-Goals

- **Player-readable board contents.** No UI panel listing posts. The knowledge overlay shows aggregate information flow. The board is a simulation mechanism.
- **Villager-to-villager relay of board contents.** Villagers do not tell each other "the garrison says wolves are east." Information transfers only at buildings. Villager-to-villager encounter sharing (from `per_villager_memory.md` future extensions) would transfer personal memories, not board contents.
- **Board consensus or voting.** Buildings do not synthesize posts into decisions. Individual villagers read individual posts and make individual decisions. Aggregate behavior emerges from many individuals reacting to the same board.
- **Player writing to boards.** The player influences the settlement through building placement, not by posting orders. This aligns with the anti-goal: "no micromanagement."

## Implementation Order

1. **Add `BuildingBoard` component to all building entities at construction time.** Board type matches `BuildingType`. No behavior changes yet.
2. **Implement garrison board: write `PredatorSighting` from fleeing villagers, read during soldier patrol.** This is the most impactful single board because it changes visible military behavior.
3. **Implement danger forwarding.** Predator sightings written to any building forward to the nearest garrison.
4. **Implement workshop board: `MaterialNeeded` / `ProductionStalled` / `OutputReady`.** Workers write on state changes. Gatherers read on delivery visits.
5. **Implement gatherer AI integration.** Gatherers who read workshop boards factor `MaterialRequest` into gather priority scoring.
6. **Implement granary board: `FoodNeeded` / `WinterWarning`.** Tie into season system for automatic warning generation.
7. **Implement farm board: auto-updating crop status.** Simplest board, mostly informational for passing villagers.
8. **Add building-specific knowledge overlay modes.** Extend the `o` overlay cycle with garrison/workshop/granary views.
9. **Tune staleness thresholds and urgency scaling through playtesting.** Numbers in this doc are starting estimates.
