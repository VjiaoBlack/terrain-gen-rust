# Information Sharing on Encounter

**Status:** Proposed
**Pillar:** 2 (Emergent Complexity from Simple Agents)
**Phase:** Economy Depth (Phase 2)
**Depends on:** Per-villager memory, Stockpile bulletin board, Spatial hash grid
**Enables:** Full ant-colony information flow, emergent scouting value, road-as-information-highway

---

## Problem

With per-villager memory and the stockpile bulletin board, information flows through the settlement -- but only through the stockpile. A villager who discovers a stone vein to the east must walk all the way back to the stockpile before anyone else can learn about it. If two villagers pass each other on the road -- one heading east looking for stone, the other heading west having just found stone -- they walk right past each other. The discoverer continues to the stockpile. The seeker continues east, wastes time exploring, and eventually finds the same vein on their own (or doesn't).

This is wrong. Two people standing next to each other should talk.

The stockpile bulletin board handles settlement-scale knowledge (Layer 3 "hub" communication). What is missing is Layer 3 "peer" communication: villagers who are physically near each other exchange what they know. Information spreads through the population via contact, not teleportation to a central node.

### Why this matters for emergence

The bulletin board creates a star topology -- all information flows through the center. Encounter sharing creates a mesh topology -- information flows along whatever paths villagers happen to walk. This produces richer emergent behavior:

- A road between the settlement and a distant mine becomes an information highway. Villagers traveling the road encounter each other and share knowledge. More traffic means faster information spread. This reinforces the "roads emerge from traffic" design without any explicit information-routing code.
- Clusters of villagers working the same area (farmers in a field, builders at a site) form local knowledge pools. A farmer who spots wolves nearby tells the adjacent farmers immediately, not after someone walks to the stockpile.
- Explorers who cross paths pool their discoveries. Two explorers heading in different directions meet at a crossroads and each learns what the other found. Double the exploration coverage per explorer life.

## Design

### What counts as an encounter

Two villagers are in an **encounter** when they are within **3 tiles** of each other.

Why 3 tiles:
- It is well within sight range (22 tiles), so both villagers can see each other.
- It is close enough to represent "stopping to talk" rather than "saw someone in the distance."
- On a 16x16 spatial hash cell, a 3-tile encounter radius means we only need to check villagers within the same cell and its immediate neighbors. In practice, most encounters happen within a single cell.
- At typical walking speed (~1 tile/tick at base speed), two villagers heading toward each other are within 3 tiles for roughly 2-3 ticks, which is enough time for the system to catch the encounter at least once.

The encounter check uses squared distance to avoid a sqrt: `dx*dx + dy*dy <= 9.0`.

### What gets shared

When two villagers encounter each other, they exchange their **highest-confidence memories that the other villager lacks**. This is not a full memory dump -- it is a bounded exchange that mimics a brief conversation.

**Per encounter, each villager transfers up to `MAX_SHARE_PER_ENCOUNTER` (3) entries to the other.** The entries are selected by descending confidence from the sharer's memory, filtered to entries the receiver does not already know.

Shareable memory kinds:
- `WoodSource` -- "I saw trees over there"
- `StoneSource` -- "I saw stone over there"
- `FoodSource` -- "I saw food over there"
- `DangerZone` -- "I saw wolves over there"
- `BuildSite` -- "There is a build site that needs work"
- `ResourceDepleted` -- "That forest is gone, don't bother" (from the bulletin board doc's `PostKind`)

Not shared:
- `HomeLocation` -- personal, pinned, not useful to others.
- `StockpileLocation` -- pinned, every villager already knows this from spawn.
- `VisitedArea` -- too low-value, would flood the exchange with exploration breadcrumbs.
- `believed_stockpile` -- stockpile resource counts are shared only at the stockpile itself. Secondhand reports of "I think there was 4 wood in the stockpile 300 ticks ago" are too unreliable and would add complexity for little value.

**Shared entries are marked as secondhand.** The receiver gets the entry with `firsthand: false` and a confidence penalty:

```rust
const ENCOUNTER_CONFIDENCE_PENALTY: f32 = 0.15;

// Received entry confidence = sharer's confidence - penalty, clamped to [0.1, 0.9]
let received_confidence = (sharer_confidence - ENCOUNTER_CONFIDENCE_PENALTY)
    .clamp(0.1, 0.9);
```

This means:
- Firsthand knowledge (confidence ~1.0) transfers well (received at ~0.85).
- Already-degraded knowledge (confidence ~0.5) transfers poorly (received at ~0.35, near stale threshold).
- Third-hand knowledge (A told B, B tells C) arrives at ~0.7, then ~0.55 -- it propagates but degrades with each hop. After 4-5 hops, the information is near the stale threshold and stops spreading naturally.

This confidence chain is the "telephone game" mechanic. Information from the source is reliable. Information that passed through 5 villagers is vague. The player's solution: build stockpile outposts so explorers can post firsthand reports to a board, and other villagers read the board directly instead of relying on multi-hop gossip.

### Deduplication: "do I already know this?"

Before transferring an entry, check whether the receiver already has a memory of the same `MemoryKind` within **5 tiles** of the entry's location (the same upsert radius used in per-villager memory observation). If so, skip it -- the receiver already knows about that resource area. If the receiver's existing entry has lower confidence, update it to the transferred confidence instead of skipping (the sharer has fresher information).

```rust
fn should_accept(receiver: &VillagerMemory, entry: &MemoryEntry) -> AcceptResult {
    if let Some(existing) = receiver.find_near(entry.kind, entry.x, entry.y, 5.0) {
        if existing.confidence >= entry.confidence {
            AcceptResult::Skip  // I already know this, and my info is fresher
        } else {
            AcceptResult::Update  // I know this area but your info is fresher
        }
    } else {
        AcceptResult::Insert  // I didn't know about this at all
    }
}
```

### Encounter cooldown

Two specific villagers who just exchanged information should not re-exchange every tick while they remain near each other. Each villager tracks a small **encounter cooldown set**: the entity IDs of villagers they recently shared with, and when.

```rust
const ENCOUNTER_COOLDOWN_TICKS: u64 = 60;  // ~1 minute of game time
const MAX_COOLDOWN_ENTRIES: usize = 8;      // ring buffer, oldest evicted

pub struct EncounterCooldown {
    entries: [(Entity, u64); MAX_COOLDOWN_ENTRIES],  // (entity_id, tick_of_encounter)
    len: usize,
}
```

Before initiating a share, check: "have I shared with this villager in the last 60 ticks?" If yes, skip. This means two villagers who are working in the same area (e.g., adjacent farm plots) share once and then leave each other alone for a while. If one of them leaves and comes back 100 ticks later with new information, they share again.

The 8-entry ring buffer is deliberately small. A villager forgets who they talked to after 8 new encounters, which is fine -- the cooldown exists to prevent per-tick spam, not to track long-term social relationships.

Memory cost: 8 entries * 16 bytes = 128 bytes per villager. At 500 villagers = 64 KB. Negligible.

## Avoiding O(n^2): Spatial Hash Grid Integration

The naive approach -- check every pair of villagers for proximity -- is O(n^2). At 500 villagers, that is 125,000 pair checks per tick. Unacceptable.

The spatial hash grid (16x16 cells) solves this completely. The encounter system iterates **cells, not villagers**:

```rust
fn system_encounter_sharing(
    grid: &SpatialHashGrid,
    world: &mut hecs::World,
    tick: u64,
) {
    // Iterate each cell in the grid
    for cell_idx in 0..grid.cell_count() {
        let villagers_in_cell: SmallVec<[SpatialEntry; 16]> = grid
            .entries_in_cell_by_index(cell_idx)
            .iter()
            .filter(|e| e.categories & category::VILLAGER != 0)
            .copied()
            .collect();

        // Only check pairs within the same cell
        // (3-tile encounter radius is well within a 16-tile cell)
        for i in 0..villagers_in_cell.len() {
            for j in (i + 1)..villagers_in_cell.len() {
                let a = &villagers_in_cell[i];
                let b = &villagers_in_cell[j];
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                if dx * dx + dy * dy <= 9.0 {
                    try_share(world, a.entity, b.entity, tick);
                }
            }
        }
    }
}
```

**Why same-cell only, not adjacent cells?** The encounter radius is 3 tiles. The cell size is 16 tiles. Two villagers in adjacent cells can be at most ~1 tile apart (on the cell boundary) or as far as ~31 tiles apart (opposite edges). We would need to check adjacent cells to catch villagers on opposite sides of a cell boundary who are within 3 tiles of each other.

However, the probability of missing an encounter on a cell boundary is low. Two villagers within 3 tiles of a cell boundary will likely be within 3 tiles in the *next* tick after one of them crosses into the other's cell. For correctness without over-engineering, check the **same cell only** for the initial implementation. If boundary misses become a measured problem, extend to checking adjacent cells for villagers within 3 tiles of a cell edge -- but this is unlikely to matter in practice because villagers move and the grid rebuilds every tick.

### Cost analysis

At 500 villagers spread across 256 cells:
- Average ~2 villagers per cell
- Pair checks per cell: C(2, 2) = 1
- Total pair checks: ~256
- With clustering (20 villagers near stockpile in one cell): C(20, 2) = 190 pairs in that cell
- Worst realistic case (stockpile cluster + spread): ~400 pair checks total

Compare to naive O(n^2): 125,000 pair checks. The grid gives a **~300x reduction**.

Each pair check that results in a share does O(MEMORY_CAPACITY) = O(32) work to scan memories and find transferable entries. At ~400 pairs, that is ~12,800 comparisons in the worst case. Well under 1ms.

| Population | Pair checks (grid) | Pair checks (naive) | Speedup |
|------------|-------------------|---------------------|---------|
| 30         | ~15               | 435                 | 29x     |
| 100        | ~60               | 4,950               | 83x     |
| 500        | ~400              | 124,750             | 312x    |
| 1000       | ~1,200            | 499,500             | 416x    |

### Tick frequency

The encounter system does not need to run every tick. Information sharing is not time-critical. Running every **5 ticks** cuts cost by 5x and is unnoticeable to the player -- a 5-tick delay in sharing is ~0.08 seconds at 60fps.

Combined with tick budgeting: offscreen villagers who are already thinking every 3-5 ticks can have their encounters checked at the same reduced frequency. The encounter system piggybacks on the existing tick budget.

## Integration with Existing Systems

### Per-villager memory (Layer 2)

Encounter sharing reads from and writes to `VillagerMemory`. The `MemoryEntry` struct already has everything needed -- `kind`, `x`, `y`, `tick_observed`, `confidence`. No changes to the data structure.

Shared entries use the existing `upsert` logic with the 5-tile dedup radius. Capacity limits and eviction rules apply normally -- if a villager's memory is full, the lowest-confidence entry is evicted to make room for incoming shared knowledge, same as any other memory insertion.

### Stockpile bulletin board (Layer 3 hub)

Encounter sharing complements the bulletin board, it does not replace it. The bulletin board is persistent (posts survive for 5000 ticks) and accessible to any villager who visits. Encounter sharing is ephemeral (only happens when two villagers are near each other) and personal (only the two participants benefit).

The key difference: bulletin board posts are firsthand-only (villagers only post what they personally saw). Encounter shares include secondhand knowledge. This means the bulletin board is higher quality (all firsthand) but slower (requires a stockpile visit). Encounters are lower quality (may be multi-hop) but faster (happens wherever villagers meet).

A villager who learns something via encounter and later visits the stockpile will NOT post it to the bulletin board (because `firsthand: false`). Only the original observer can post it. This prevents secondhand rumors from polluting the board.

### Spatial hash grid

The encounter system is the spatial hash grid's first non-AI consumer. It uses the grid that is already rebuilt every tick in `system_ai`. No additional grid construction is needed. The system reads the grid after `system_ai` populates it but before or after AI runs -- ordering does not matter because encounters affect next-tick behavior, not current-tick decisions.

### Danger zone spreading

Encounter sharing of `DangerZone` entries creates emergent alarm behavior. A villager who sees wolves flees toward the settlement. On the way, they encounter outbound villagers and share the danger sighting. Those villagers now know about the wolves and reroute or flee. The alarm propagates along the traffic flow at the speed of villager movement, not instantaneously.

This is the "ant alarm pheromone" behavior described in the game design doc. A wolf attack creates a visible wave of fleeing/rerouting villagers that spreads outward from the encounter point.

## Constants Summary

```rust
const ENCOUNTER_RADIUS: f64 = 3.0;              // tiles
const ENCOUNTER_RADIUS_SQ: f64 = 9.0;           // precomputed for fast checks
const MAX_SHARE_PER_ENCOUNTER: usize = 3;        // entries transferred per direction
const ENCOUNTER_CONFIDENCE_PENALTY: f32 = 0.15;  // confidence lost on transfer
const ENCOUNTER_COOLDOWN_TICKS: u64 = 60;        // ticks before re-sharing with same villager
const MAX_COOLDOWN_ENTRIES: usize = 8;            // ring buffer size
const ENCOUNTER_SYSTEM_FREQUENCY: u64 = 5;        // run every N ticks
```

## Observable Behavior (Pillar 4)

What the player should see after this system is implemented:

1. **Road encounters change behavior.** A villager heading east passes a villager heading west on the road. The westbound villager just found stone. The eastbound villager, who was exploring aimlessly, suddenly changes direction toward the stone. Cause and effect visible on screen.

2. **Alarm cascading.** A villager spots wolves and flees west. They pass three outbound villagers on the road. All three turn around. A fourth villager, further out and past the encounter point, keeps walking east obliviously -- they were not contacted. The player sees the information wavefront.

3. **Cluster learning.** An explorer returns to a group of farmers near a field. The explorer passes through the group. All farmers in the group learn what the explorer found. One farmer who was idle (no crops ready) leaves to gather the newly-learned resource. The others keep farming because they are busy.

4. **Isolated villagers stay ignorant.** A lone miner at a distant quarry never encounters other villagers (no one walks that far). They have stale memories and may not know about a new food source or danger zone that the main settlement figured out 500 ticks ago. Building a road to the quarry increases foot traffic, which increases encounters, which keeps the miner informed.

5. **Multi-hop degradation visible.** A resource location discovered at confidence 1.0 by villager A is shared to B (0.85), then B encounters C (0.70), then C encounters D (0.55). By D, the confidence is near stale. D might walk to the location and find it valid, refreshing to 1.0 (firsthand). Or D might ignore it because the score is too low. The player sees some villagers acting on the information and others not.

## Edge Cases

**Two villagers encounter each other with nothing new to share.** The cooldown check fires first (O(1)), so the cost is negligible. If they have not shared recently, the memory scan finds no transferable entries and returns immediately. No wasted work.

**Large group encounter (20 villagers at stockpile).** 190 pair checks. Each pair likely has nothing to share (they all just read the bulletin board and have the same knowledge). The dedup check (`should_accept` -> `Skip`) short-circuits quickly. Cost: ~190 * O(3 entries * 32 memory scan) = ~18,000 comparisons. Under 0.1ms. And this only happens every 5 ticks.

**Villager encounters while fleeing.** A fleeing villager (BehaviorState::FleeHome) participates in encounters normally. They can share their DangerZone knowledge on the way home. They can also receive knowledge, but they will not act on it until they finish fleeing. This is correct behavior -- you can yell "wolves!" while running.

**Encounter between idle and busy villager.** Both participate equally. The idle villager may learn about a resource and immediately seek it. The busy villager (e.g., hauling wood) will not change behavior mid-haul but will have the knowledge for their next decision.

**Death during encounter tick.** If a villager dies between the grid snapshot and the encounter system running, the `world.get` call for their memory will fail. Guard with `if let Ok(...)` and skip dead entities. The grid snapshot is stale by definition (it was built at the start of the tick), so entities that despawned mid-tick are expected.

## Performance Budget

| Component | Per encounter pair | Per tick (500 pop, 5-tick freq) |
|-----------|-------------------|-------------------------------|
| Pair distance check | ~2 ns (multiply + compare) | ~80 ns (400 pairs / 5) |
| Cooldown check | ~10 ns (scan 8 entries) | ~800 ns |
| Memory scan for shareable entries | ~200 ns (scan 32 entries, pick top 3) | ~16 us (80 sharing pairs) |
| Memory insertion (upsert) | ~50 ns (dedup check + insert) | ~4 us |
| **Total** | | **~21 us per active tick** |

At 5-tick frequency, amortized cost is ~4 us per tick. Negligible compared to AI (5ms budget at 500 pop) and pathfinding (3ms budget).

## Testing Strategy

**Unit tests:**
- `test_encounter_share_basic`: Two villagers within 3 tiles. A has a WoodSource memory B lacks. After encounter, B has the WoodSource with reduced confidence.
- `test_encounter_share_confidence_penalty`: Shared entry arrives at `original - 0.15` confidence.
- `test_encounter_share_max_entries`: Sharer has 10 entries receiver lacks. Only 3 (highest confidence) are transferred.
- `test_encounter_dedup_skip`: Receiver already knows about the location. No duplicate created.
- `test_encounter_dedup_update`: Receiver has stale knowledge (confidence 0.3), sharer has fresh (0.9). Receiver's entry is updated.
- `test_encounter_cooldown`: Two villagers share. On the next tick, they do not share again. After 60 ticks, they share again.
- `test_encounter_out_of_range`: Two villagers at distance 4.0. No sharing occurs.
- `test_encounter_no_home_sharing`: Sharer has HomeLocation memory. It is not transferred.
- `test_encounter_secondhand_not_posted`: Villager learns WoodSource via encounter (firsthand: false). Visits stockpile. The bulletin board does NOT receive that entry.

**Integration tests:**
- `test_info_spreads_along_road`: Place 5 villagers in a line, 2 tiles apart. Give villager 0 a unique StoneSource memory. Run 10 ticks. Assert that the memory propagates down the line with decreasing confidence at each hop.
- `test_danger_alarm_cascade`: Spawn a predator near villager A. A spots it, creates DangerZone. A flees past villagers B and C. Assert B and C receive DangerZone memory within ~5 ticks of A passing them.
- `test_isolated_villager_uninformed`: Villager A is 50 tiles from any other villager. Villager B discovers a resource. After 100 ticks, A still does not know about it (no encounter occurred). A learns only when they visit the stockpile.

**Behavioral tests (multi-thousand tick runs):**
- After 10,000 ticks, the number of unique resource locations known by the average villager should be higher than with bulletin-board-only sharing (compare with encounter system disabled).
- DangerZone entries should reach 80% of villagers within 200 ticks of a predator sighting near the settlement core, vs 500+ ticks with bulletin-board-only.
- Villagers on high-traffic roads should have higher average memory entry counts than villagers in isolated areas.

## Implementation Order

1. Add `EncounterCooldown` struct to `components.rs`. Attach to all villager entities at spawn.
2. Add `system_encounter_sharing` to `systems.rs`. Wire into `Game::step()` after `system_ai`, gated by tick frequency (every 5 ticks).
3. The system iterates spatial grid cells, finds villager pairs within encounter radius, checks cooldowns, and calls the sharing logic.
4. Sharing logic: scan sharer's memory for shareable entries (filtered by kind, sorted by confidence), check receiver dedup, insert with confidence penalty.
5. Mark shared entries as `firsthand: false` so they are excluded from bulletin board posting.
6. Run full test suite. Verify no performance regression at current population.
7. Add diagnostic: count encounters per tick, average entries shared per encounter. Expose in debug overlay.

## Future Extensions

- **Building-mediated sharing.** Villagers inside the same building (workshop, garrison) share knowledge automatically, as if in constant encounter. The garrison becomes a threat intelligence hub: soldiers share DangerZone entries. The workshop shares resource sightings. Uses the same sharing logic but triggered by building co-occupancy instead of proximity.
- **Verbal range vs. encounter range.** Different memory kinds could have different sharing ranges. DangerZone (shouting "wolves!") could share at 8 tiles. Resource sightings (casual conversation) at 3 tiles. Adds flavor but increases system complexity.
- **Group abstraction.** When 5+ villagers are within encounter range of each other, treat them as a "knowledge group" that shares a pooled memory. Avoids O(n^2) pair checks within the group. Relevant at 1000+ pop with large work crews. The spatial grid's per-cell villager list is already the seed data for group detection.
- **Visual indicator.** A brief flash or speech-bubble glyph when two villagers share information. In map mode, a `!` appears over the receiver for 1 tick. Gives the player visibility into information flow without cluttering the screen.
