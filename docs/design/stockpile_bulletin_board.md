# Stockpile as Bulletin Board (Knowledge Sharing Hub)

*Design doc for Pillar 2, Layer 3: Settlement-level knowledge sharing*

## Problem

Today, `SettlementKnowledge` is a god-object that every villager reads every tick. It is recomputed globally every 100 ticks by scanning a fixed radius around the settlement centroid (`update_settlement_knowledge` in `game/build.rs`). There is no concept of a villager *learning* something or *reporting* something. A villager who walks 40 tiles east and discovers a stone vein doesn't tell anyone — the settlement either knows (if it's within the 20-tile known_radius) or doesn't. Information doesn't flow through the world; it appears from a global scan.

This violates the core principle of Pillar 2: **information is local, spreads through the world, not through global variables.**

## Solution: The Bulletin Board

The stockpile becomes a physical location where knowledge is exchanged. Villagers who visit the stockpile (which they already do every hauling trip) both **deposit resources** and **read/write the bulletin board**. The bulletin board is a data structure attached to the stockpile entity that holds reports from villagers about what they've seen in the world.

A villager who discovers a stone vein while exploring writes a report when they next visit the stockpile. Another villager who is idle and looking for work reads the bulletin board and learns about the stone vein. Information spreads at the speed of villager foot traffic, not at the speed of a global tick function.

## Data Structures

### BulletinBoard (attached to each Stockpile entity)

```rust
/// A single report posted to a stockpile's bulletin board.
#[derive(Debug, Clone)]
pub struct BulletinPost {
    pub kind: PostKind,
    pub location: (usize, usize),  // world tile coordinates
    pub posted_tick: u64,          // when this was posted
    pub reporter: Entity,          // who posted it (for staleness tracking)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostKind {
    /// "There's a stone deposit at (x, y)"
    ResourceSighting { resource: ResourceType },
    /// "There's a berry bush at (x, y)"
    FoodSource,
    /// "I saw wolves near (x, y)"
    DangerZone,
    /// "The forest at (x, y) is gone — I went there and it was stumps"
    ResourceDepleted { resource: ResourceType },
    /// "There's open fertile land near (x, y) for farming"
    FertileLand,
    /// "There's an unfinished build site at (x, y)"
    BuildSiteNeedsWork,
}

/// The bulletin board attached to a stockpile entity.
#[derive(Debug, Clone, Default)]
pub struct BulletinBoard {
    pub posts: Vec<BulletinPost>,
}
```

### VillagerMemory (per-villager component, new)

```rust
/// What a single villager personally knows about the world.
#[derive(Debug, Clone, Default)]
pub struct VillagerMemory {
    /// Locations this villager has personally seen or learned from the bulletin board.
    /// Each entry has a tick timestamp for staleness.
    pub known_resources: Vec<MemoryEntry>,

    /// Locations where this villager saw danger.
    pub danger_zones: Vec<MemoryEntry>,

    /// Tick when this villager last visited a stockpile (read the board).
    pub last_stockpile_visit: u64,

    /// Maximum entries before oldest are evicted (bounded memory).
    pub capacity: usize, // default: 16
}

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub kind: PostKind,
    pub location: (usize, usize),
    pub learned_tick: u64,
    /// True if learned firsthand (villager saw it). False if learned from bulletin board.
    pub firsthand: bool,
}
```

### Capacity and eviction

`VillagerMemory::capacity` defaults to 16 entries. When a villager learns something new and their memory is full, the oldest entry is evicted. This creates natural information loss: a villager who has been exploring for a long time may have forgotten early discoveries by the time they return. Only the bulletin board preserves that knowledge permanently (until it goes stale).

The bulletin board itself has a soft cap of 64 posts per stockpile. Posts older than 5000 ticks are pruned automatically each time the board is written to. `ResourceDepleted` posts cancel earlier `ResourceSighting` posts for the same location.

## Read/Write Protocol

### Writing to the board (villager arrives at stockpile)

A villager writes to the bulletin board when they transition into the stockpile's vicinity (distance < 1.5) during a `Hauling` or `Eating` (from stockpile food) state. This is the moment they currently deposit resources.

**What gets written:**
1. Any `ResourceSighting` entries from the villager's personal memory that are `firsthand: true` and not already on the board (dedup by location + kind).
2. Any `DangerZone` entries from personal memory.
3. Any `ResourceDepleted` entries (the villager went to a known location and found it empty).

**Pseudocode:**
```rust
fn write_to_board(board: &mut BulletinBoard, memory: &VillagerMemory, 
                  villager: Entity, current_tick: u64) {
    for entry in &memory.known_resources {
        if !entry.firsthand { continue; } // only post what you saw yourself
        if board.has_post_at(entry.location, &entry.kind) { continue; } // already known
        board.posts.push(BulletinPost {
            kind: entry.kind,
            location: entry.location,
            posted_tick: current_tick,
            reporter: villager,
        });
    }
    // ResourceDepleted posts remove contradicting ResourceSighting posts
    board.posts.retain(|p| {
        !memory.known_resources.iter().any(|e| {
            matches!(e.kind, PostKind::ResourceDepleted { .. })
            && e.location == p.location
            && matches!(p.kind, PostKind::ResourceSighting { .. })
        })
    });
    board.prune_stale(current_tick, 5000);
}
```

### Reading from the board (villager arrives at stockpile)

Reading happens in the same moment as writing. After writing, the villager scans the board and copies any posts they don't already know into their `VillagerMemory`.

**What gets read:**
1. All `ResourceSighting` posts — the villager now knows where resources are.
2. All `DangerZone` posts — the villager will avoid those areas.
3. `ResourceDepleted` posts — the villager removes corresponding stale entries from their own memory.

**Pseudocode:**
```rust
fn read_from_board(board: &BulletinBoard, memory: &mut VillagerMemory, 
                   current_tick: u64) {
    for post in &board.posts {
        if memory.already_knows(post.location, &post.kind) { continue; }
        memory.learn(MemoryEntry {
            kind: post.kind,
            location: post.location,
            learned_tick: current_tick,
            firsthand: false, // learned secondhand from the board
        });
    }
    // Evict stale personal entries contradicted by ResourceDepleted posts
    memory.known_resources.retain(|e| {
        !board.posts.iter().any(|p| {
            matches!(p.kind, PostKind::ResourceDepleted { .. })
            && p.location == e.location
        })
    });
}
```

### When personal memory is created (Layer 1 -> Layer 2)

A villager creates `firsthand` memory entries when:

1. **Gathering completes** — the villager just gathered wood/stone. They remember the location and resource type. If the resource is now depleted (`ResourceYield::remaining == 0`), they create a `ResourceDepleted` entry instead.
2. **Exploring reveals terrain** — during `BehaviorState::Exploring`, every N ticks the villager checks tiles within sight range. Forest tiles become `ResourceSighting { Wood }`, mountain tiles become `ResourceSighting { Stone }`, berry bushes become `FoodSource`.
3. **Predator spotted** — when a villager enters `FleeHome` because of a nearby predator, they create a `DangerZone` entry at the predator's approximate location.
4. **Arriving at a stale location** — a villager walks to a remembered forest location and finds it's now grass (deforested). They create `ResourceDepleted { Wood }` and remove the old sighting from their memory.

## Integration with AI Decision-Making

### Replacing global knowledge reads

Currently `ai_villager()` receives `stockpile_wood`, `stockpile_stone`, `stone_deposits`, and `frontier` as parameters computed from global state. The migration path:

**Phase 1 (bulletin board exists, AI still reads globals):**
- Add `BulletinBoard` component to stockpile entities.
- Add `VillagerMemory` component to villager entities.
- On hauling deposit, villagers read/write the board (side effect only, AI doesn't use it yet).
- The `update_settlement_knowledge()` function continues running. The board populates in parallel.
- Tests can verify board contents match global knowledge within a tolerance.

**Phase 2 (AI reads personal memory, globals become fallback):**
- `ai_villager()` gains a `memory: &VillagerMemory` parameter.
- Resource-seeking logic checks `memory.known_resources` first instead of scanning `stone_deposits` within sight range.
- If memory has no leads, fall back to sight-range scan (Layer 1).
- If sight-range scan finds nothing, fall back to global `frontier` for exploration.
- `update_settlement_knowledge()` frequency drops from every 100 ticks to every 500 ticks (emergency fallback only).

**Phase 3 (globals removed):**
- `SettlementKnowledge` struct removed entirely.
- All resource-seeking is memory-driven or sight-driven.
- Exploration targets come from the bulletin board's absence of data ("nobody has reported what's to the east" -> explore east).
- `update_settlement_knowledge()` deleted.

### How a villager picks what to gather

Today: check global stockpile counts, find nearest resource within sight range, go gather.

After bulletin board:
1. Check personal hunger (Layer 1, unchanged).
2. Read personal memory for known resource locations (Layer 2).
3. Score each known location by: distance, resource type urgency (stockpile counts still visible at the stockpile when the villager last visited), and staleness (how many ticks since the sighting).
4. If no memory leads, scan sight range (Layer 1 fallback).
5. If nothing visible, check bulletin board posts they remember for unexplored directions (Layer 3).
6. If truly nothing, wander or explore frontier.

### How exploration becomes valuable

Today: exploration is driven by `resources_scarce && !frontier.is_empty()`. The frontier is a ring of tiles computed globally.

After bulletin board:
- Explorers who find resources create `firsthand` memory entries.
- When the explorer returns to the stockpile (to eat, to deposit), they post discoveries to the board.
- Other villagers read the board and learn the locations.
- **Time delay is real:** if an explorer dies before returning, their discoveries die with them. This makes exploration risky and valuable. Losing a good explorer means losing knowledge.
- Frontier tiles aren't computed globally. Instead, the settlement "knows" the edges of explored territory because that's where the most recent exploration sightings are. Unexplored directions are directions with no board posts.

## Interaction with Multiple Stockpiles

The game already supports building additional stockpiles. Each stockpile has its own `BulletinBoard`. When a villager visits stockpile A, they read/write stockpile A's board. They do NOT automatically know what's on stockpile B's board.

This creates emergent geography in knowledge flow:
- A mining outpost with its own stockpile shares knowledge among miners but not with the main settlement until a hauler carries resources (and knowledge) back.
- Building a stockpile near a frontier is not just a resource cache — it's an information relay.
- Two distant stockpiles may have contradictory information (one says "forest at X", the other says "forest at X is depleted") until a villager visits both.

For Phase 1, if only one stockpile exists (the default), this is equivalent to a single global board.

## Performance

**Memory cost:**
- `BulletinBoard`: 64 posts x ~40 bytes = ~2.5 KB per stockpile. Negligible.
- `VillagerMemory`: 16 entries x ~32 bytes = ~512 bytes per villager. At 500 villagers = ~250 KB. Fine.

**CPU cost per tick:**
- Board read/write only happens when a villager arrives at a stockpile (state transition from `Hauling` to `Idle`). This is O(board_size) per arrival, which is rare per-villager (once every ~200-400 ticks).
- Memory scanning during AI decision is O(memory_capacity) = O(16) per villager per tick. Cheaper than the current O(stone_deposits) scan.
- Board pruning is O(board_size) and happens only on writes.

**Scale at 500 villagers:**
- ~2-3 stockpile arrivals per tick on average (500 villagers / ~200 tick hauling cycle).
- Each arrival does O(64) board scan + O(16) memory scan = O(80) work.
- Total per tick: O(240). Negligible compared to pathfinding.

## Observable Behavior (Pillar 4)

The bulletin board should create **visible** information flow:

- A villager who just read the board and learned about a distant stone vein should immediately change behavior: stop wandering, start seeking the stone. The player sees cause and effect — "that villager visited the stockpile and then walked northeast. Someone must have reported stone up there."
- Villagers who haven't visited the stockpile recently continue with stale knowledge. They might walk to a depleted forest and find nothing. The player sees the inefficiency: "that villager is going to an empty forest — they haven't been to the stockpile lately."
- An explorer returns and deposits. Multiple idle villagers at the stockpile simultaneously learn and fan out toward the new discovery. The player sees a burst of purposeful movement after a delivery — the "pheromone" spreading.

**Potential overlay:** A "knowledge" overlay mode (added to the `o` cycle) that color-codes tiles by how many villagers know about them. Bright = widely known. Dark = only one villager knows. Uncolored = nobody knows. This makes information flow visible.

## Emergent Consequences

Things that happen naturally from this system without extra code:

1. **Information bottleneck at stockpile.** If the stockpile is far from resource sites, knowledge spreads slowly. Building a closer stockpile (outpost) speeds up information flow AND resource delivery. One mechanic serves two purposes.

2. **Explorer death = knowledge loss.** A villager dies to wolves before returning to the stockpile. Their `VillagerMemory` is destroyed. The stone vein they found stays undiscovered until another explorer finds it. The player feels the loss beyond just population count.

3. **Natural scouting value.** Explorers who survive and return are disproportionately valuable because they bring back multiple sightings at once. The board gets several new posts, multiple villagers learn, and a wave of activity follows.

4. **Stale knowledge creates visible mistakes.** A villager acts on 3000-tick-old information and walks to a depleted forest. They find nothing, create a `ResourceDepleted` post in their memory, return to the stockpile, post it, and now other villagers won't make the same mistake. The self-correcting loop is visible.

5. **Rush of activity after hauling.** Several villagers arrive at the stockpile in the same few ticks (hauling is periodic). They all read the board, all learn the same new information, and fan out simultaneously. This creates the "ant colony" burst pattern — a pulse of coordinated movement originating from the stockpile.

6. **Multiple stockpiles create information regions.** The east-side stockpile knows about eastern resources; the west-side stockpile knows about western resources. A villager who visits both becomes a knowledge bridge. This emerges without any "information region" code.

## Testing Strategy

**Unit tests (in `ecs/mod.rs`):**
- `test_bulletin_board_write_dedup`: posting the same sighting twice doesn't create duplicates.
- `test_bulletin_board_depleted_cancels_sighting`: a `ResourceDepleted` post removes the corresponding `ResourceSighting`.
- `test_bulletin_board_prune_stale`: posts older than threshold are removed.
- `test_villager_memory_capacity`: adding beyond capacity evicts oldest entry.
- `test_villager_memory_learn_from_board`: reading a board with 5 posts adds 5 entries to empty memory.
- `test_villager_memory_firsthand_vs_secondhand`: only `firsthand` entries get posted to the board.

**Integration tests (in `tests/integration.rs`):**
- `test_knowledge_spreads_through_stockpile`: spawn 2 villagers. Villager A explores east and finds stone. Villager A returns to stockpile. Villager B visits stockpile. Assert villager B's memory now contains the stone location.
- `test_explorer_death_loses_knowledge`: spawn explorer, give them a sighting, kill them before they reach stockpile. Assert the board does NOT contain their sighting.
- `test_stale_knowledge_correction`: post a resource sighting. Later, a villager visits the location and finds it empty. They return and post `ResourceDepleted`. Assert the original sighting is removed from the board.

**Behavioral tests (run game for N ticks, check diagnostics):**
- After 5000 ticks, every resource location within 30 tiles of the settlement should appear on at least one stockpile's bulletin board.
- Villagers with recent stockpile visits (last 200 ticks) should have more memory entries on average than villagers who haven't visited recently.
- No villager should seek a resource location that has a `ResourceDepleted` post on the board they last read.

## Non-Goals

- **Villager-to-villager knowledge transfer on encounter.** This is the "Rich" tier from the game design doc. The bulletin board is the "Core" tier. Encounter-based sharing is a future extension that uses the same `VillagerMemory` data structure but triggers on proximity instead of stockpile visit.
- **Building-specific boards** (garrison shares threat intel, workshop shares recipes). Future extension. The data structures support `PostKind` expansion, but only the stockpile acts as a hub in this design.
- **Player-readable board UI.** No text panel showing board contents. The knowledge overlay (color-coded tiles) is the player-facing visualization. The board is an internal simulation mechanism.

## Implementation Order

1. Add `VillagerMemory` component to villager entities at spawn. Add `BulletinBoard` component to stockpile entities at spawn. No behavior changes yet.
2. Hook into the `Hauling -> Idle` transition in `ai_villager()`: on arrival at stockpile, call `write_to_board()` then `read_from_board()`.
3. Hook into `Exploring` state: on sight-range scan, populate `VillagerMemory` with firsthand sightings.
4. Hook into `Gathering` completion: record resource location (or depletion) in memory.
5. Hook into `FleeHome` trigger: record danger zone in memory.
6. (Phase 2) Modify `ai_villager()` resource-seeking to check `VillagerMemory` before sight-range scan.
7. (Phase 3) Remove `SettlementKnowledge` and `update_settlement_knowledge()`.
