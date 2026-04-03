# Feature: Outpost Mechanics
Pillar: Explore > Expand > Exploit > Endure (#3), Geography Shapes Everything (#1)
Priority: Rich (Phase 3 deliverable, depends on resource map + terrain-driven settlement)

## What

Small satellite settlements that form organically near distant resource deposits. An outpost consists of a stockpile and a shelter, connected to the main settlement by an established road. Villagers rotate between the main settlement and the outpost on a shift schedule. Outposts create visible territorial expansion, supply line logistics, and strategic vulnerability -- a road cut by predators or terrain collapse isolates the outpost and forces a crisis response.

## Why

Right now, distant resources are simply far away. A villager walks 80 tiles to a stone vein, mines one unit, walks 80 tiles back. The round trip takes hundreds of ticks, and nothing interesting happens along the way. There is no sense of expansion, no territorial footprint, no supply chain to protect or optimize.

The game design doc lists "outposts, resource colonies" as a Phase 3 deliverable under Scale, and "outpost mechanics: small satellite settlements near distant resources, connected by roads" as a Rich feature under the Explore-Expand-Exploit-Endure pillar. Outposts solve three problems simultaneously:

1. **Economic efficiency.** Villagers at a remote outpost stockpile spend most of their time gathering, not walking. Resources accumulate at the outpost stockpile and are carried back in bulk by haulers. Total throughput per villager-tick increases dramatically.
2. **Visible expansion.** A settlement with three outposts connected by roads looks like a civilization claiming territory. The player can see supply lines, traffic flow, and the settlement's geographic reach. This is Pillar 4 (Observable Simulation) in action.
3. **Strategic vulnerability.** An outpost connected by a single road through wolf territory is a liability. Cutting the road (predator attack, seasonal flooding, deforestation blocking the path) isolates the outpost. Villagers there are stranded with limited food. This creates emergent tension without scripted events.

## Current State

**No outpost system exists.** All buildings are placed by `auto_build_tick` using the main settlement centroid. There is no concept of a secondary settlement center or remote stockpile.

**Relevant existing systems:**

- **`TrafficMap`** tracks foot traffic per tile. Roads auto-form when traffic exceeds a threshold. Villagers walking to distant resources already create visible trails -- this is the precursor to outpost supply roads.
- **`InfluenceMap`** tracks settlement territory as a radial field from buildings. Currently single-center. Would need to support multiple influence sources for outposts.
- **`SettlementKnowledge`** stores known resource locations discovered by exploration. An outpost decision needs to read this to know which distant deposits are worth the investment.
- **`auto_build_tick`** has a priority-ordered building queue. Outpost placement would be a new entry in this queue, triggered by specific conditions.
- **`find_building_spot`** (being replaced by terrain-driven scoring in `terrain_driven_settlement.md`) places buildings near the centroid. Outpost placement is fundamentally different -- it places buildings near a *remote resource*, not near the center.
- **`ResourceMap`** (from `precomputed_resource_map.md`) provides ground-truth deposit locations and richness. This is the data source for deciding *where* an outpost is worth building.
- **Villager AI states** (`Behavior` enum in `components.rs`) include `Seeking`, `Gathering`, `Hauling`, `Returning`. Outpost staffing introduces new states or modifies existing ones.

## Design

### When Does an Outpost Form?

An outpost is not manually placed. It emerges from a **trigger condition** evaluated during `auto_build_tick`:

```
outpost_trigger(deposit_location) =
    distance_from_stockpile(deposit_location) > OUTPOST_MIN_DISTANCE (30 tiles)
    AND deposit.remaining > OUTPOST_MIN_RICHNESS (40 units)
    AND known_to_settlement (in SettlementKnowledge)
    AND traffic_to_deposit > OUTPOST_TRAFFIC_THRESHOLD (villagers have walked there >= 20 times)
    AND population >= OUTPOST_MIN_POP (15 villagers)
    AND no existing outpost within OUTPOST_EXCLUSION_RADIUS (25 tiles) of deposit
    AND settlement has surplus resources (wood >= 30, stone >= 15, food >= 20)
```

**Reading the trigger:** The settlement does not speculatively build outposts. It builds one when villagers have *already* been trekking to a distant resource often enough that the traffic map shows a worn path. The outpost formalizes what the villagers are already doing. This follows the design principle that roads emerge from traffic -- outposts emerge from sustained remote gathering.

The `OUTPOST_TRAFFIC_THRESHOLD` is the key gate. It means the settlement has to discover a resource, send gatherers to it repeatedly, and only after sustained use does the outpost trigger. This naturally sequences with the Explore-Expand arc: exploration finds the resource, early gathering proves its value, the outpost cements the expansion.

### What Buildings Does an Outpost Need?

An outpost is minimal. It has exactly two required buildings and one optional:

| Building | Purpose | Cost | Notes |
|----------|---------|------|-------|
| **Outpost Stockpile** | Local resource storage | 5w 3s | Smaller capacity than main stockpile (max 50 per resource type). Acts as drop-off point for gatherers and pickup point for haulers. |
| **Shelter** | Overnight rest, weather protection | 6w 2s | Smaller than a Hut. Houses 2-3 villagers. Villagers without shelter at an outpost suffer fatigue and return to main settlement early. |
| **Outpost Garrison** (optional) | Defense against predators | 3w 5s 1m | Auto-built if the outpost is in or near a threat zone (wolf territory within 15 tiles). Without it, outpost villagers flee to the main settlement on any predator sighting, abandoning the outpost temporarily. |

**Placement logic:** The outpost stockpile is placed at the highest-scoring walkable tile within 5 tiles of the target resource deposit, using terrain-driven scoring (flatness + proximity to the deposit). The shelter is placed within 8 tiles of the outpost stockpile, preferring the side closest to the main settlement (so villagers heading home pass by it). The garrison, if triggered, is placed between the outpost and the nearest known threat direction.

**New `BuildingType` variants:**

```rust
BuildingType::OutpostStockpile { parent_stockpile: Entity, target_deposit: (i32, i32) }
BuildingType::Shelter { capacity: u8 }
// Garrison already exists; outpost garrisons use the same type.
```

The `parent_stockpile` link lets the hauling system know where to carry resources. The `target_deposit` records which resource deposit this outpost was built to exploit.

### Outpost Data Structure

```rust
pub struct Outpost {
    pub id: u32,
    pub stockpile_entity: Entity,
    pub shelter_entity: Entity,
    pub garrison_entity: Option<Entity>,
    pub target_deposit: (i32, i32),
    pub deposit_type: DepositType,
    pub assigned_gatherers: Vec<Entity>,    // villagers staffing this outpost
    pub assigned_haulers: Vec<Entity>,      // villagers running supply between outpost and main
    pub road_intact: bool,                  // false if path to main settlement is blocked
    pub established_tick: u64,              // when the outpost was founded
}
```

Stored on `Game` as `pub outposts: Vec<Outpost>`. An outpost is a logical grouping, not a single entity.

### Villager Staffing: Who Goes, and When?

Villagers are never manually assigned. The AI system selects outpost staff based on availability and proximity:

**Gatherer assignment:**

```
For each outpost with assigned_gatherers.len() < target_staff_count:
    target_staff_count = deposit.remaining.clamp(1, 3)  // 1-3 based on richness
    
    Find idle villagers (Behavior::Idle) at the main settlement
    who are not assigned to another outpost
    and are not the last 5 villagers (reserve for main settlement operations)
    
    Assign the closest idle villager as an outpost gatherer.
```

**Gatherer behavior cycle:**

1. **Travel to outpost** -- walk along the road to the outpost stockpile.
2. **Gather** -- behave like a normal gatherer but deposit resources at the *outpost stockpile* instead of the main stockpile.
3. **Rest at shelter** -- at night or when fatigued, sleep at the outpost shelter.
4. **Rotation** -- after `OUTPOST_SHIFT_LENGTH` ticks (default 200, roughly 2-3 days), the gatherer walks back to the main settlement and becomes idle. Another villager may be assigned to replace them.

Rotation ensures that no single villager is permanently exiled. It also means outpost knowledge (what's depleted, where threats appeared) flows back to the main settlement through returning villagers -- reinforcing the ant-colony knowledge model from Pillar 2.

**Hauler assignment:**

```
For each outpost where outpost_stockpile.total_resources > HAUL_THRESHOLD (10 units):
    If no hauler currently assigned or hauler is idle at main settlement:
        Assign closest idle villager as hauler.
```

**Hauler behavior cycle:**

1. **Walk to outpost stockpile** along the road.
2. **Pick up resources** -- take up to `HAULER_CARRY_CAPACITY` (5 units) from the outpost stockpile.
3. **Walk back to main stockpile** -- deposit resources.
4. **Repeat** while the outpost stockpile has resources.

Haulers are the supply line. They are the visible traffic on the road between settlement and outpost. Their movement is what makes the supply line observable (Pillar 4).

### Road Connection

An outpost requires a connected road to the main settlement. The road does not need to exist before the outpost is built -- the traffic from early gatherers (pre-outpost) will have already worn a path. When the outpost is established, the system verifies that a walkable path exists via BFS/A* from the outpost stockpile to the main stockpile.

**Road formation:** The existing `TrafficMap` -> road auto-build system handles this. Once an outpost formalizes the traffic pattern, the increased hauler traffic will reinforce the road. No special road-building logic is needed -- the outpost just increases traffic on an already-forming path.

**Road integrity check:** Every `ROAD_CHECK_INTERVAL` ticks (default 100), the outpost verifies path connectivity to the main stockpile. This is a BFS reachability check, not a full A* -- just "can I get there?" The result is stored as `outpost.road_intact`.

### What Happens If the Road Is Cut?

A road can be cut by:
- **Predator occupation** -- wolves camping on the road (villagers reroute or refuse to pass).
- **Seasonal flooding** -- river swells and submerges road tiles adjacent to waterways.
- **Terrain change** -- deforestation removes a forest tile the road passed through and regrowth blocks the new terrain (unlikely but possible with future terrain mutation).
- **Building obstruction** -- a wall or building placed on the road (player error in manual build mode).

When `road_intact` flips to `false`:

1. **Haulers abort.** Any hauler en route to the outpost turns around and returns to the main settlement. Resources already at the outpost stockpile stay there -- they cannot be retrieved until the road is restored.

2. **Gatherers are stranded.** Villagers at the outpost continue gathering and depositing locally, but they cannot rotate home. They will consume food from the outpost stockpile to survive. If the outpost stockpile has no food, they begin starving.

3. **Starvation timer.** Stranded villagers with no food at the outpost stockpile will attempt to pathfind home through alternate routes (if any exist). If no alternate path is found within `STRANDED_PATIENCE` ticks (150), they enter `Behavior::Fleeing` toward the main settlement, ignoring normal movement constraints (moving through Forest/Mountain at reduced speed but not stopping).

4. **Settlement response.** When the main settlement detects `road_intact == false`, it:
   - Stops assigning new gatherers/haulers to that outpost.
   - If the obstruction is predators, the existing threat system may dispatch garrison defenders along the road (if a garrison exists and has capacity).
   - The outpost remains in the `outposts` list but is marked inactive. It reactivates automatically when road connectivity is restored.

5. **Outpost decay.** An outpost with `road_intact == false` for more than `OUTPOST_DECAY_TICKS` (1000 ticks, roughly 2 weeks game-time) is abandoned. Its buildings remain on the map as ruins (visual storytelling -- Pillar 1, terrain tells history). The outpost is removed from `Game.outposts`. If the road is later restored and the deposit still has resources, a new outpost can form through the normal trigger conditions.

**This creates an emergent narrative arc:** the player sees a thriving outpost, then wolves settle near the road, haulers stop coming, the outpost slowly starves, villagers flee through the wilderness, and eventually the buildings stand empty. All from simple rules, no scripted events.

### Outpost Lifecycle Summary

```
1. DISCOVERY    — Villagers explore, find distant resource deposit
2. INFORMAL     — Gatherers trek to deposit and back, traffic wears a path
3. TRIGGER      — Traffic threshold met, resources available → auto_build_tick places outpost
4. CONSTRUCTION — Outpost stockpile + shelter built (1-2 auto-build cycles)
5. OPERATIONAL  — Gatherers rotate in, haulers carry resources back, road strengthens
6. DEPLETION    — Deposit runs out → gatherers assigned elsewhere → outpost goes idle
7. ABANDONMENT  — No gathering activity for OUTPOST_IDLE_TICKS (500) → outpost decommissioned
                   OR road cut for OUTPOST_DECAY_TICKS (1000) → outpost abandoned
8. RUINS        — Buildings remain as terrain features, can be rebuilt if new deposits found nearby
```

### Influence Map Extension

The `InfluenceMap` currently radiates from a single center. Outpost stockpiles become secondary influence sources:

```rust
// In influence map computation:
for outpost in &game.outposts {
    if outpost.road_intact {
        influence.add_source(outpost.stockpile_position, OUTPOST_INFLUENCE_RADIUS);
    }
}
```

`OUTPOST_INFLUENCE_RADIUS` is smaller than the main settlement's (8 tiles vs 20+). This means outpost influence is a small bubble around the remote stockpile, connected to the main settlement's influence along the road. The visual effect on the influence overlay is a settlement with "tendrils" reaching toward resources -- exactly the geographic expansion the design pillars call for.

### Integration Points

| System | Change | File |
|--------|--------|------|
| `Game` struct | Add `outposts: Vec<Outpost>` field | `game/mod.rs` |
| `auto_build_tick` | Add outpost trigger evaluation after existing build priorities | `game/build.rs` |
| `BuildingType` | Add `OutpostStockpile` and `Shelter` variants | `ecs/components.rs` |
| `find_building_spot` | New variant: `find_outpost_spot(deposit_pos)` that scores near a remote deposit instead of centroid | `game/build.rs` |
| `ai_villager()` | Handle `OutpostGatherer` and `OutpostHauler` behavior states | `ecs/ai.rs` |
| `Behavior` enum | Add `OutpostGathering { outpost_id }`, `Hauling { from_outpost, carrying }` | `ecs/components.rs` |
| `InfluenceMap` | Support multiple influence sources | `simulation.rs` |
| `system_hunger` | Outpost villagers eat from outpost stockpile, not main | `ecs/systems.rs` |
| Road check | Periodic BFS connectivity test per outpost | `game/build.rs` or new `game/outpost.rs` |
| Rendering | Draw outpost stockpile and shelter with distinct glyphs. Show supply line on traffic overlay. | `game/render.rs` |
| Save/Load | Serialize `Vec<Outpost>` with entity references | `game/save.rs` |

## Edge Cases

**Deposit depletes before outpost is built.** The outpost trigger checks `deposit.remaining > OUTPOST_MIN_RICHNESS`. If gatherers deplete it below threshold before the traffic threshold is met, the outpost simply never forms. The villagers were already exploiting the deposit informally -- the outpost would have been marginal anyway.

**Two deposits near each other.** The `OUTPOST_EXCLUSION_RADIUS` (25 tiles) prevents two outposts from overlapping. If two deposits are both within 25 tiles, one outpost serves both. Gatherers at that outpost can work either deposit. If the deposits are far enough apart (both > 25 tiles from each other and > 30 tiles from main), two separate outposts form.

**Outpost stockpile overflows.** The outpost stockpile has a small capacity (50 per resource type). If haulers are not keeping up, gatherers at the outpost go idle when the stockpile is full. This self-regulates: idle gatherers rotate home sooner, reducing the outpost population until hauling catches up. No explicit overflow logic needed.

**All villagers assigned to outposts.** The gatherer assignment rule reserves the last 5 villagers for main settlement operations. With a population of 15 (the minimum for outpost trigger), only 10 are eligible for outpost duty. With 3 gatherers per outpost, this supports 3 outposts max at population 15. At population 50, the reserve is still 5, allowing 15 outpost gatherers across multiple outposts. The reserve count could scale with population later if needed.

**Multiple outpost supply lines share a road segment.** Two outposts in the same general direction will have overlapping road segments near the main settlement. This is fine -- shared road segments get more traffic, reinforcing them. The haulers for each outpost are independent; they just happen to walk the same path for part of the trip.

**Player demolishes outpost building.** If the player manually demolishes an outpost stockpile or shelter in build mode, the outpost is immediately decommissioned. Assigned villagers return to main settlement. This gives the player an escape valve if an outpost is a net drain.

**Outpost in seasonal flood zone.** If the outpost stockpile is placed near a river and spring flooding submerges it, treat this like a road cut -- the outpost goes inactive until water recedes. The shelter should be placed on higher ground (terrain-driven scoring prefers elevation slightly above the deposit for shelter placement).

**Save/load with active outposts.** `Outpost` struct contains `Entity` references which need to survive serialization. The existing ECS serialization system in `serialize.rs` maps entities to stable IDs. Outpost entity references use the same mapping.

## Test Criteria

### Unit tests

1. **Outpost trigger fires correctly.** Create a game state with a known deposit at distance 40 from stockpile, traffic count 25, population 20, surplus resources. Assert `should_create_outpost()` returns true for that deposit.

2. **Outpost trigger rejects near deposits.** Same setup but deposit is 20 tiles from stockpile (< OUTPOST_MIN_DISTANCE). Assert trigger returns false.

3. **Outpost trigger rejects low population.** Population 10, all other conditions met. Assert trigger returns false.

4. **Outpost trigger rejects low traffic.** Traffic count 5 to deposit, all other conditions met. Assert trigger returns false.

5. **Exclusion radius prevents stacking.** Create an outpost at position (100, 100). Assert trigger returns false for a deposit at (110, 110) (within exclusion radius) but true for (130, 130) (outside).

6. **Road cut detection.** Create an outpost with a known path. Block a tile on the path (set to Water). Run road integrity check. Assert `road_intact == false`.

7. **Road restoration.** From test 6, unblock the tile. Run road integrity check. Assert `road_intact == true` and outpost reactivates.

8. **Gatherer rotation.** Assign a gatherer to an outpost. Advance OUTPOST_SHIFT_LENGTH ticks. Assert the gatherer's behavior transitions to returning to main settlement.

9. **Hauler triggers on stockpile threshold.** Set outpost stockpile to 12 wood (> HAUL_THRESHOLD). Run hauler assignment. Assert a hauler is assigned.

10. **Outpost abandonment on depletion.** Set outpost target deposit to `remaining: 0`. Advance OUTPOST_IDLE_TICKS. Assert outpost is removed from `game.outposts`.

### Integration tests

11. **Outpost forms organically.** Run a seed with a known distant stone deposit for 5000 ticks with auto-build. Assert an outpost exists near the stone deposit, with a stockpile and shelter entity.

12. **Supply line is visible.** After outpost formation, check that the `TrafficMap` shows elevated traffic along the path between main stockpile and outpost stockpile.

13. **Road cut causes starvation pressure.** Form an outpost, then block the road. Advance 200 ticks. Assert outpost villagers have reduced hunger (consuming local food) and eventually attempt to flee home.

14. **Outpost decommissions cleanly.** Deplete the target deposit fully. Advance 600 ticks past depletion. Assert outpost is removed, all assigned villagers are back at main settlement with `Behavior::Idle`.

15. **No regressions.** All existing tests pass. Outpost system does not interfere with base settlement behavior when no outpost trigger conditions are met.

### Visual validation (manual)

16. Run seed 42 for 10000 ticks. Observe whether outposts form at distant resource deposits. Verify that supply roads are visible on the traffic overlay. Verify that outpost buildings appear distinct from main settlement buildings.

## Dependencies

| Dependency | Status | Blocking? |
|-----------|--------|-----------|
| `ResourceMap` (precomputed_resource_map.md) | Design complete, not implemented | Yes -- outpost trigger needs deposit locations and richness |
| Terrain-driven settlement (terrain_driven_settlement.md) | Design complete, not implemented | Partial -- outpost building placement uses the scoring system, but could use simpler logic initially |
| `TrafficMap` | Exists and functional | No |
| `InfluenceMap` | Exists, single-source | Needs extension for multi-source, low effort |
| Road auto-build from traffic | Exists and functional | No |
| `SettlementKnowledge` | Exists, needs population from resource map | Yes -- outpost trigger reads known deposits |
| A* / BFS pathfinding | Exists | No |
| Save/load serialization | Exists | Needs extension for `Vec<Outpost>` |

## Estimated Scope

| Task | Effort | Notes |
|------|--------|-------|
| `Outpost` struct + `Vec<Outpost>` on `Game` | 1 hour | Data structures, outpost lifecycle states |
| `OutpostStockpile` + `Shelter` building types | 2 hours | New BuildingType variants, placement logic, rendering glyphs |
| Outpost trigger evaluation in `auto_build_tick` | 3 hours | Traffic analysis, deposit scoring, distance/population/surplus checks |
| `find_outpost_spot` placement near remote deposit | 2 hours | Variant of terrain-driven scoring centered on deposit, not centroid |
| Gatherer AI: `OutpostGathering` behavior | 3 hours | Travel to outpost, gather locally, deposit at outpost stockpile, rotation |
| Hauler AI: `Hauling` behavior | 2 hours | Walk to outpost, pick up resources, walk back, deposit at main |
| Road integrity check (periodic BFS) | 1 hour | Connectivity test, `road_intact` flag management |
| Road-cut response: hauler abort, stranded logic, flee | 3 hours | Starvation at outpost, alternate pathfinding, decay timer |
| Influence map multi-source support | 1 hour | Add outpost stockpiles as secondary influence sources |
| Outpost depletion/abandonment/ruins lifecycle | 2 hours | Idle detection, decommission, building-to-ruin conversion |
| Rendering: outpost glyphs, supply line on overlay | 1 hour | Distinct stockpile/shelter glyphs, traffic overlay shows supply lines |
| Save/load for outposts | 1 hour | Serialize `Vec<Outpost>` with entity ID mapping |
| Unit tests (items 1-10) | 3 hours | Mock game states with known terrain/deposits |
| Integration tests (items 11-15) | 3 hours | Multi-thousand-tick runs with outpost assertions |
| Tuning: trigger thresholds, shift length, carry capacity | 2-4 hours | Playtest across seeds, adjust constants |
| **Total** | **30-34 hours** | Spread across ~5-6 sessions |

### Implementation order

1. `Outpost` struct, `OutpostStockpile`/`Shelter` building types (data layer).
2. Outpost trigger evaluation -- depends on `ResourceMap` and `SettlementKnowledge` being populated.
3. `find_outpost_spot` placement logic.
4. Gatherer AI for outpost duty (travel, gather locally, rotate).
5. Hauler AI (supply line between outpost and main).
6. Road integrity check + road-cut response.
7. Influence map multi-source.
8. Outpost lifecycle: depletion, abandonment, ruins.
9. Unit + integration tests.
10. Threshold tuning across seeds.
