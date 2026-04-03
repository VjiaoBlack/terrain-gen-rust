# Memory Decay

**Status:** Proposed
**Pillar:** 2 (Emergent Complexity from Simple Agents)
**Phase:** Foundation / Economy Depth boundary
**Depends on:** Per-Villager Memory (per_villager_memory.md)
**Enables:** Re-scouting behavior, information freshness as emergent pressure, visible inefficiency from stale knowledge

---

## Problem

Per-villager memory (per_villager_memory.md) introduces a `confidence` field that decays at a flat rate (`MEMORY_DECAY_RATE = 0.002` per tick). This is a placeholder. A flat linear decay does not model how information actually goes stale in a living world:

- A forest 10 tiles from the settlement is likely still there after 500 ticks. A forest 80 tiles away that the villager saw 2000 ticks ago could be clearcut by now.
- "There is stone in the mountains" stays true for thousands of ticks because mountains don't move. "There is food at that berry bush" goes stale in a few hundred ticks because someone else probably ate it.
- A villager who walks to a remembered forest, finds stumps, and turns around is currently a silent failure. It should be a visible, legible event that the player can watch and understand.

The current flat decay treats all memories the same. It cannot distinguish between volatile information (food, danger) and stable information (stone deposits, terrain). It does not create the observable "wasted trip" behavior described in the per-villager memory doc. And it provides no mechanism for a villager to realize their memory is wrong and correct it.

## Design

### Core Concept

Memory decay is not a single number. Each memory kind has its own **half-life** based on how likely the real world has changed since the observation. Decay accelerates based on distance from the settlement center (far-away things change without you knowing). When a villager acts on stale memory and discovers reality has changed, they visibly react, correct their memory, and may post a `ResourceDepleted` report to the bulletin board.

### Decay Rates by Memory Kind

Each `MemoryKind` has a base half-life: the number of ticks before confidence drops to 0.5. Confidence follows exponential decay: `confidence = initial * 0.5^(age / half_life)`.

| MemoryKind | Base Half-Life (ticks) | Rationale |
|---|---|---|
| HomeLocation | never (pinned) | Your home does not move |
| StockpileLocation | never (pinned) | Stockpiles are permanent structures |
| StoneSource | 3000 | Stone deposits deplete slowly; mountains never move |
| WoodSource | 1200 | Forests get cut down by other villagers, but regrow |
| FoodSource | 400 | Berry bushes are eaten quickly; farms harvest on cycles |
| BuildSite | 600 | Build sites get completed or abandoned |
| DangerZone | 250 | Predators move; danger is highly transient |
| VisitedArea | 800 | Terrain changes slowly, but exploration value fades |

These half-lives mean:
- A villager remembers a stone deposit with >0.5 confidence for ~50 in-game minutes (at 60 ticks/min). Reliable for a long time.
- A food source memory drops below the stale threshold (0.3) after roughly 550 ticks (~9 minutes). Villagers who rely on old food knowledge will frequently find nothing.
- A danger zone memory becomes unreliable after ~350 ticks (~6 minutes). Villagers stop avoiding a wolf sighting area within a reasonable time, but not instantly.

### Distance Modifier

Memories of locations far from the villager's current position decay faster. The intuition: if you are near the forest, you would notice if someone started cutting it down. If you are across the map, you have no way of knowing.

```
effective_half_life = base_half_life * distance_factor

distance_factor = 1.0 / (1.0 + distance_to_memory / DISTANCE_DECAY_SCALE)
```

Where `DISTANCE_DECAY_SCALE = 60.0` (tiles). This means:
- Memory of something 10 tiles away: `factor = 0.86`, half-life barely reduced.
- Memory of something 60 tiles away: `factor = 0.50`, half-life halved.
- Memory of something 120 tiles away: `factor = 0.33`, half-life cut to a third.

A villager on the far side of the map forgets distant resource locations much faster than a villager working nearby. This creates natural pressure to stay near known resources or to re-scout distant ones.

### Constants

```rust
/// Per-kind base half-lives (ticks until confidence = 0.5)
const HALFLIFE_STONE_SOURCE: u64 = 3000;
const HALFLIFE_WOOD_SOURCE: u64 = 1200;
const HALFLIFE_FOOD_SOURCE: u64 = 400;
const HALFLIFE_BUILD_SITE: u64 = 600;
const HALFLIFE_DANGER_ZONE: u64 = 250;
const HALFLIFE_VISITED_AREA: u64 = 800;

/// Distance at which effective half-life is halved
const DISTANCE_DECAY_SCALE: f64 = 60.0;

/// Below this confidence, memory is "stale" — villager treats it as unreliable
const STALE_THRESHOLD: f32 = 0.3;

/// Below this confidence, memory is evicted on next cleanup
const FORGET_THRESHOLD: f32 = 0.05;

/// Ticks a villager pauses when arriving at a stale location (visible "confusion")
const STALE_ARRIVAL_PAUSE: u64 = 8;
```

### Decay Computation

Replace the current flat `confidence -= MEMORY_DECAY_RATE` with exponential decay computed from the age of the memory:

```rust
impl MemoryEntry {
    /// Compute current effective confidence based on age and distance.
    fn effective_confidence(&self, current_tick: u64, villager_x: f64, villager_y: f64) -> f32 {
        let base_half_life = self.kind.half_life();
        if base_half_life == 0 { return self.confidence; } // pinned

        let age = (current_tick - self.tick_observed) as f64;
        let dist = ((villager_x - self.x).powi(2) + (villager_y - self.y).powi(2)).sqrt();
        let distance_factor = 1.0 / (1.0 + dist / DISTANCE_DECAY_SCALE);
        let effective_half_life = base_half_life as f64 * distance_factor;

        let decay = (0.5_f64).powf(age / effective_half_life);
        (self.confidence as f64 * decay) as f32
    }
}
```

**Performance note:** `powf` is called per-entry per-query, not per-tick. Since `best_resource` already iterates entries, this adds one `powf` per entry per query. At 32 entries, this is negligible. If profiling shows otherwise, we can cache the effective confidence once per tick and reuse it.

### What Triggers a Refresh

A memory's confidence resets to 1.0 and its `tick_observed` updates to the current tick when:

1. **Direct observation.** The villager is within sight range of the location and the resource is still there. The existing observation system in `system_update_memories` already handles this via upsert.

2. **Arrival verification.** The villager arrives at a remembered location and the resource is still present. Confidence snaps to 1.0. This is stronger than a distant sighting.

3. **Bulletin board read.** The villager visits a stockpile and reads a post about a resource at a location they already remember. If the post is newer than their memory, the memory refreshes to the post's confidence. (Depends on stockpile_bulletin_board.md.)

4. **Encounter sharing.** A nearby villager shares a fresher memory of the same location. (Future, Layer 3.)

A memory is **NOT** refreshed by:
- Wishing. Staying far away and hoping the forest is still there does nothing.
- Global state changes. Even if the forest objectively still exists, the villager's confidence keeps decaying until they or someone else verifies it.

### What Triggers Correction (Stale Arrival)

The interesting case: a villager walks to a remembered resource location and discovers reality has changed. This is the "wasted trip" that makes memory decay observable.

**Detection.** When a villager in `Seeking` or `Gathering` state arrives at their target location (within 2 tiles) and the expected resource is not there:

- WoodSource target, but the tile is now Grass/Stump (deforested)
- StoneSource target, but the deposit entity is gone (mined out)
- FoodSource target, but the berry bush is empty or the farm has no pending harvest
- BuildSite target, but the building is complete or demolished

**Correction sequence:**

```
1. Villager enters "Confused" micro-state (new: not a full behavior state,
   just a pause flag + animation cue).
   Duration: STALE_ARRIVAL_PAUSE ticks (8 ticks, ~0.13 seconds at 60fps).

2. During pause, the villager:
   a. Removes or zeroes the stale memory entry.
   b. Observes current surroundings (normal observation scan).
   c. If a replacement resource is visible within sight range, creates a
      new memory and redirects toward it.
   d. If nothing suitable is visible, transitions to Exploring.

3. If the villager has a WoodSource memory that was wrong, and they later
   visit a stockpile, they post a ResourceDepleted bulletin so other
   villagers stop walking to the same spot.
```

### Observable Behavior

These are the player-visible consequences of memory decay. Each one should be legible without any UI overlay, per Pillar 4.

**1. The Wasted Trip**

A villager walks purposefully toward a forest that was cut down 500 ticks ago. They arrive, pause (the confusion micro-state), look around, and either redirect to a nearby alternative or turn back. The player sees: a villager walk somewhere, stop, and change course. This reads as "they went to gather wood but the forest was gone."

At normal game speed, this happens naturally every few minutes as the settlement grows and consumes resources. It is not a bug; it is a signal that the settlement's knowledge is stale and could benefit from more scouts or closer stockpiles.

**2. The Knowledge Gap After Death**

The settlement's best explorer dies to wolves. They were the only villager who had recently visited the northern forest. Other villagers' memories of that forest are ancient (low confidence). For the next few hundred ticks, nobody goes north. Eventually someone explores in that direction and "rediscovers" it. The player sees: a period of reduced gathering efficiency after losing an experienced villager.

**3. The Freshness Gradient**

Villagers working near a resource have constantly-refreshed memories (confidence near 1.0). Villagers across the settlement have decaying memories of that same resource. When the nearby workers exhaust it, they immediately know and redirect. Distant villagers keep walking toward it for a while because their memory says it is there. The player sees: nearby villagers adapt instantly, distant villagers lag behind. Information travels at foot speed.

**4. The Scout's Value**

A villager who regularly patrols the map perimeter keeps their memories fresh. They rarely make wasted trips. A villager who has been farming for 2000 ticks straight has ancient resource memories and will be confused when they finally need to gather. The player sees: specialists are efficient at their job but useless when reassigned. Generalists adapt faster. This is not scripted; it falls out of who has fresh memories.

**5. Danger Fading**

After a wolf attack from the east, all nearby villagers have DangerZone memories. For ~350 ticks, they avoid eastern resources. The danger memory fades. Villagers start drifting east again. If the wolves are actually gone, this is correct adaptation. If the wolves are still there, the villagers will see them again and refresh the danger memory. The player sees: a period of avoidance followed by cautious return, without any explicit "fear timer."

**6. Seasonal Food Scramble**

At the start of winter, FoodSource memories from autumn berry bushes go stale fast (400-tick half-life). Villagers who relied on berry bushes walk to empty bushes, pause in confusion, and redirect to the stockpile or to farms. The player sees: a brief period of disorganization at seasonal transitions as villagers learn that food sources have changed.

### Interaction With Other Systems

**Bulletin Board (stockpile_bulletin_board.md):** Stale arrivals generate `ResourceDepleted` posts. These are high-value information because they prevent other villagers from making the same wasted trip. A settlement with a well-visited stockpile self-corrects stale knowledge faster. A settlement with distant, isolated workers corrects slowly.

**Exploration (per_villager_memory.md):** Memory decay is the primary driver of re-exploration. When all remembered WoodSource entries drop below `STALE_THRESHOLD`, the villager has no confident wood locations and transitions to exploring. The settlement naturally generates scouts: villagers whose knowledge has gone stale.

**Traffic and Roads:** Roads accelerate information freshness indirectly. Faster travel means villagers visit the stockpile more often (refreshing believed_stockpile), make gathering trips faster (refreshing resource memories), and encounter each other more frequently (future: encounter sharing). A road to a distant resource is not just a speed boost; it is an information channel.

**Spatial Hash Grid (spatial_hash_grid.md):** The distance modifier in decay computation needs the villager's current position, which is already available. No spatial queries required. The observation scan that refreshes memories on sight will benefit from the spatial grid for entity checks.

## Migration Path

### Step 1: Replace flat decay with per-kind exponential decay

- Replace `MEMORY_DECAY_RATE` constant with per-kind half-lives.
- Replace `confidence -= MEMORY_DECAY_RATE` in `decay_tick()` with `effective_confidence()` computation.
- Add distance modifier.
- **No behavior change yet** -- just different decay curves. Run existing tests, verify memories decay at reasonable rates.
- **Test:** WoodSource memory at distance 0 has confidence >0.5 at tick 1200. DangerZone memory at distance 0 has confidence <0.3 at tick 350. StoneSource at distance 120 decays 3x faster than at distance 0.

### Step 2: Add stale arrival detection and confusion micro-state

- In the `Seeking` behavior, when the villager arrives at target and the resource is missing, trigger the correction sequence.
- Add `confused_until: Option<u64>` field to the villager AI state. While `current_tick < confused_until`, the villager pauses in place.
- On confusion clear: remove stale entry, re-observe, redirect or explore.
- **Test:** Spawn a villager with a WoodSource memory at a location that is actually grass. Villager walks there, pauses for STALE_ARRIVAL_PAUSE ticks, then transitions to exploring. Memory entry is removed.

### Step 3: Generate ResourceDepleted bulletin posts

- When a villager corrects a stale memory, queue a `ResourceDepleted` post for their next stockpile visit.
- Other villagers reading the bulletin board remove or downgrade matching memories.
- **Test:** Villager A has stale WoodSource at (50, 20). Villager A walks there, finds stumps, returns to stockpile, posts ResourceDepleted. Villager B visits stockpile, reads post, removes their WoodSource memory at (50, 20).

### Step 4: Tune and observe

- Run 10K-tick simulations and count wasted trips per 1000 ticks. Target: 2-5 wasted trips per 1000 ticks at 30 pop. Too many means decay is too fast; too few means decay is too slow.
- Watch for degenerate cases: all villagers confused simultaneously, no one gathering. Adjust half-lives if needed.
- Check that stone memories stay useful for a long time (miners should not get confused about mountain locations).

## Edge Cases

**All resources depleted in an area.** A villager arrives at a remembered forest, finds stumps, observes nothing useful nearby. They transition to exploring. If the entire region is deforested, they will explore further and further out. This is correct: it creates the visible "search spiral" that motivates expansion.

**Very new villager.** Newborns have no resource memories. They explore immediately. Decay is irrelevant until they have memories to decay. No special case needed.

**Villager stuck in a loop.** A villager remembers forest at A (stale, confidence 0.35) and forest at B (stale, confidence 0.33). They walk to A, find nothing, walk to B, find nothing, walk to A again. **Prevention:** On stale arrival, the memory is removed (not just decayed further). The villager cannot loop back to a location they have already debunked. After both are removed, they explore.

**Pinned memories point to destroyed buildings.** HomeLocation and StockpileLocation are pinned (do not decay). If a hut is destroyed by a raid, the villager's HomeLocation memory is now wrong. This is handled separately from decay: building destruction should explicitly notify assigned villagers and clear the pin. This is out of scope for memory decay but noted here.

**Concurrent deforestation.** Multiple villagers remember the same forest. Three of them walk there. The first one chops the last trees. The second arrives, finds stumps, gets confused, corrects. The third is still walking, arrives later, also gets confused. Each independently corrects. This is realistic and observable: a wave of villagers arriving at a depleted site and turning around one by one.

## Performance Budget

| Component | Per villager | 30 villagers | 500 villagers |
|-----------|-------------|-------------|---------------|
| Decay computation (per query, 32 entries) | ~0.5 us | ~15 us | ~250 us |
| Stale arrival check (1 per arrival) | ~0.2 us | negligible | negligible |
| Confusion pause (no computation, just skip) | 0 | 0 | 0 |
| Bulletin post generation | ~0.1 us | negligible | negligible |
| **Total per tick** | | **~15 us** | **~250 us** |

Decay computation is the only recurring cost, and it replaces the existing flat decay computation (which iterates the same entries). Net performance impact is near zero. The `powf` call is the only addition, and it is fast on modern hardware.

## Future Extensions

**Confidence-based pathfinding cost.** A villager choosing between two WoodSource memories could factor confidence into the decision: a nearby stale memory (short walk, might be gone) vs. a distant fresh memory (long walk, definitely there). This would make villagers prefer reliable information over proximity, creating visible "I'll go to the one I'm sure about" behavior.

**Memory sharing with confidence transfer.** When a villager tells another about a resource, the listener should receive the confidence at a discount (e.g., 0.8x the teller's confidence). Second-hand information is less reliable than first-hand. This creates value for direct observation over gossip.

**Seasonal decay modifiers.** Food memories could decay faster in winter (bushes die) and slower in summer (food is abundant and stable). This would make the seasonal food scramble more pronounced.

**Decay visualization overlay.** A debug overlay showing memory confidence as colored dots on the map for a selected villager. Fresh memories are bright, stale memories are dim, forgotten memories flash and disappear. Useful for tuning and for the player to understand why a villager is confused.
