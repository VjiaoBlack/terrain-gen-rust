# Environmental Traces

**Status:** Proposed
**Pillar:** 2 (Emergent Complexity from Simple Agents)
**Phase:** Foundation / Economy Depth boundary
**Depends on:** TrafficMap (exists in `src/simulation.rs`), Per-Villager Memory (proposed)
**Enables:** Full pheromone system, cultural memory, emergent road networks, territorial behavior

---

## Problem

Villagers communicate through two channels: direct observation (sight range) and the stockpile bulletin board (proposed). Both require a villager to be *present* — at the stockpile, or within sight range of another villager. There is no persistent, ambient information layer. A villager walking through the world sees terrain and entities, but the terrain itself carries no history of what happened there.

The `TrafficMap` in `src/simulation.rs` already tracks accumulated foot traffic per tile and converts high-traffic tiles to roads. But this data is invisible to villager AI — it only drives a terrain conversion threshold. A villager standing at a fork in the path has no way to infer "many villagers go left, few go right" even though the simulation knows this.

Meanwhile, the world looks static between visits. A mining site that 15 villagers use daily looks the same as untouched mountain. A settlement border that villagers patrol looks the same as wilderness. The world doesn't accumulate evidence of activity, and villagers can't read evidence that isn't there.

This is the ant colony gap. Real ants navigate almost entirely through environmental traces (pheromones). Our villagers navigate through personal memory and explicit communication. Environmental traces are the missing middle layer — information that persists in the world, decays over time, and can be read by any villager who passes through.

## Design

### Core Concept

The world accumulates **traces** from villager activity. Traces are per-tile floating-point values that increase when villagers perform specific actions and decay over time. Villager AI reads nearby traces to make better decisions without needing personal memory of a location or a bulletin board report.

Traces are the ant colony pheromone analog: indirect communication through a shared environment.

### Trace Types

Six trace layers, each a 2D grid matching the map dimensions (same structure as `TrafficMap`):

| Trace | Created by | Read by | Meaning |
|-------|-----------|---------|---------|
| **Foot Traffic** | Walking (already exists) | Pathfinding, exploration | "Villagers travel here often" |
| **Gather Scent** | Picking up a resource | Resource-seeking villagers | "Someone found something useful here" |
| **Danger Scent** | Fleeing, taking damage, dying | All villagers pathing | "Something bad happened here" |
| **Home Scent** | Villagers leaving buildings | Lost/new villagers | "Civilization is in that direction" |
| **Depletion Mark** | Failing to gather (empty deposit) | Resource-seeking villagers | "This source is exhausted" |
| **Territory Mark** | Villagers idle near owned buildings | Threat AI, other settlements | "This area is claimed" |

### Data Structure

```rust
/// A single trace layer — one f64 per map tile.
/// Same layout as TrafficMap but generalized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceLayer {
    width: usize,
    height: usize,
    values: Vec<f64>,
    decay_rate: f64,       // multiplied per decay tick (e.g., 0.998)
    decay_interval: u64,   // how many ticks between decay passes
}

/// All environmental trace layers bundled together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentalTraces {
    pub foot_traffic: TraceLayer,   // NOTE: replaces or wraps existing TrafficMap
    pub gather_scent: TraceLayer,
    pub danger_scent: TraceLayer,
    pub home_scent: TraceLayer,
    pub depletion_mark: TraceLayer,
    pub territory_mark: TraceLayer,
}
```

`TraceLayer` generalizes the existing `TrafficMap` pattern. The current `TrafficMap` becomes `foot_traffic` with its existing `step_on`/`decay`/`road_candidates` behavior preserved. The five new layers follow the same structure with different decay rates and emission rules.

### Trace Lifecycle

**1. Emission**

Traces are emitted as side effects of actions villagers already perform. No new villager behaviors are needed — traces are written by existing systems.

```
Foot Traffic:   villager moves to tile         -> foot_traffic[tile] += 1.0
                (already implemented in TrafficMap::step_on)

Gather Scent:   villager picks up resource     -> gather_scent[tile] += 5.0
                villager harvests farm          -> gather_scent[tile] += 3.0

Danger Scent:   villager enters Fleeing state   -> danger_scent[tile] += 10.0
                villager takes damage           -> danger_scent[tile] += 15.0
                villager dies                   -> danger_scent[tile] += 50.0

Home Scent:     villager exits a building       -> home_scent[tile] += 2.0
                building is constructed         -> home_scent[radius 5] += 10.0

Depletion Mark: villager arrives at remembered  -> depletion_mark[tile] += 20.0
                resource location, finds nothing
                (the "wasted trip" from per_villager_memory.md)

Territory Mark: villager idles within 3 tiles   -> territory_mark[tile] += 0.5
                of an owned building (per tick)
```

Emission values are tuned so that a single event is barely readable, but repeated events create strong signals. A single villager gathering wood once leaves a faint gather scent. Twenty villagers gathering wood over 500 ticks create a beacon.

**2. Diffusion**

After emission, trace values spread slightly to neighboring tiles. This makes traces readable from adjacent tiles rather than requiring exact tile overlap.

```rust
/// Spread trace values to Moore neighbors (8-connected).
/// Each tick, each tile shares `spread_factor` of its value with neighbors.
fn diffuse(layer: &mut TraceLayer, spread_factor: f64) {
    // Implemented as a blur pass with a 3x3 kernel.
    // spread_factor = 0.05 means each tile keeps 60% (1 - 8*0.05),
    // and each neighbor gets 5%.
}
```

Diffusion rates per layer:

| Trace | Spread Factor | Effect |
|-------|--------------|--------|
| Foot Traffic | 0.0 (none) | Paths stay narrow and precise |
| Gather Scent | 0.03 | Gently widens around resource sites |
| Danger Scent | 0.06 | Spreads outward — danger feels larger than the exact spot |
| Home Scent | 0.08 | Radiates broadly from settlement, creating a gradient toward home |
| Depletion Mark | 0.01 | Stays localized to the depleted site |
| Territory Mark | 0.04 | Forms a soft boundary around clusters of buildings |

**3. Decay**

Each layer decays at its own rate, applied every `decay_interval` ticks:

| Trace | Decay Rate | Decay Interval | Half-life (approx) | Why |
|-------|-----------|----------------|---------------------|-----|
| Foot Traffic | 0.999 | 10 ticks | ~7000 ticks | Paths are persistent; roads are permanent |
| Gather Scent | 0.995 | 5 ticks | ~700 ticks | Resource sites shift as deposits deplete |
| Danger Scent | 0.990 | 5 ticks | ~350 ticks | Danger fades — wolves move on |
| Home Scent | 0.998 | 10 ticks | ~3500 ticks | Settlement is persistent but abandoned areas fade |
| Depletion Mark | 0.9995 | 10 ticks | ~14000 ticks | Exhausted sites stay marked a long time |
| Territory Mark | 0.993 | 5 ticks | ~500 ticks | Must be actively maintained by presence |

Decay is multiplicative: `values[i] *= decay_rate`. Same approach as the existing `TrafficMap::decay()`. Values below a floor (0.01) are zeroed to avoid floating-point dust.

### How AI Reads Traces

Villager AI samples traces at nearby tiles during the decision pass. Traces modify existing decisions rather than creating new behavior states. The villager doesn't "follow pheromones" — they make the same decisions they already make, but with better information.

#### 1. Pathfinding: Prefer Worn Paths

When A* computes path cost, tiles with high foot traffic get a discount:

```rust
// In A* cost calculation (currently in ecs/ai.rs)
let base_cost = terrain.movement_cost();
let traffic = traces.foot_traffic.get(nx, ny);
let traffic_discount = (traffic / 50.0).min(0.3);  // up to 30% cheaper
let cost = base_cost * (1.0 - traffic_discount);
```

This means villagers naturally prefer paths that other villagers use, which reinforces those paths, which makes them cheaper, which attracts more traffic. The positive feedback loop is the road-building mechanism — the existing `road_candidates` threshold is the crystallization point where a worn path becomes permanent infrastructure.

The discount is capped at 30% so that a terrible-terrain path (through forest, cost 1.7) with high traffic (1.7 * 0.7 = 1.19) is still more expensive than grass (1.0) but less expensive than untrafficked forest. This means paths through difficult terrain only form when there is no better alternative — exactly when a road would be most valuable.

#### 2. Exploration: Avoid Trodden Ground

Explorers currently pick random directions. With traces, they prefer tiles with LOW foot traffic:

```rust
// Explorer direction scoring
let traffic = traces.foot_traffic.get(tx, ty);
let novelty_bonus = 1.0 / (1.0 + traffic);  // high traffic = low novelty
let explore_score = base_score + novelty_bonus;
```

This creates natural exploration fanning: the first explorer goes north, wears a path, the second explorer sees the worn path north and prefers unexplored east. Scouts spread out without explicit coordination.

#### 3. Resource Seeking: Follow Gather Scent

A villager looking for resources who has no personal memory of a source can follow gather scent:

```rust
// In resource-seeking decision (ai_villager)
// After checking personal memory (per_villager_memory.md) and finding nothing:
let nearby_gather = traces.gather_scent.sample_gradient(vx, vy, radius: 8);
if let Some((gx, gy, strength)) = nearby_gather {
    // Move toward the strongest gather scent gradient
    seek_target = Some((gx, gy));
}
```

`sample_gradient` checks tiles in a radius and returns the direction of increasing scent. This is the direct pheromone-following behavior: a new villager who knows nothing can still find the wood-gathering area by following the scent trails of experienced gatherers.

#### 4. Danger Avoidance: Steer Around Danger Scent

Danger scent adds cost to pathfinding, creating soft avoidance zones:

```rust
// In A* cost calculation, stacks with traffic discount
let danger = traces.danger_scent.get(nx, ny);
let danger_penalty = (danger / 20.0).min(2.0);  // up to +2.0 cost
let cost = base_cost * (1.0 - traffic_discount) + danger_penalty;
```

After a wolf attack, the area where villagers died or fled gets a danger scent cloud. Pathfinding routes around it. As the scent decays (half-life ~350 ticks), villagers gradually resume using the area. If wolves attack again, the scent refreshes and the avoidance persists.

Unlike per-villager DangerZone memory (which only the witnessing villager knows), danger scent is readable by ALL villagers. A villager who wasn't present during the attack still avoids the area because the ground itself carries the warning.

#### 5. Lost Villagers: Follow Home Scent

A villager with no HomeLocation memory (rare edge case) or a newly arrived migrant can follow the home scent gradient back toward the settlement:

```rust
// Fallback for villagers who can't find their way home
let home_direction = traces.home_scent.sample_gradient(vx, vy, radius: 12);
if let Some((hx, hy, _)) = home_direction {
    move_toward(hx, hy);
}
```

Home scent radiates outward from buildings with high diffusion. It forms a gradient field pointing toward the settlement center. This is the "follow the smell of civilization" behavior — elegant for migrants who spawn at map edges and need to find the settlement without global knowledge of its location.

#### 6. Resource Avoidance: Read Depletion Marks

When a villager is choosing between two remembered resource sites, depletion marks break ties:

```rust
let depletion_a = traces.depletion_mark.get(ax, ay);
let depletion_b = traces.depletion_mark.get(bx, by);
// Prefer the site with less depletion marking
let score_a = memory_confidence_a - (depletion_a / 30.0);
let score_b = memory_confidence_b - (depletion_b / 30.0);
```

This solves the "wasted trip" problem from per_villager_memory.md more efficiently. Instead of every villager independently walking to a depleted forest and being disappointed, the first disappointed villager leaves a depletion mark that warns subsequent villagers. The information persists in the world, not in any single villager's head.

### TraceLayer::sample_gradient

The key query method that AI uses to follow trace signals:

```rust
impl TraceLayer {
    /// Sample tiles within `radius` and return the position of the
    /// strongest signal, plus the signal strength.  Returns None if
    /// all sampled values are below `min_threshold`.
    pub fn sample_gradient(
        &self,
        cx: usize,
        cy: usize,
        radius: usize,
        min_threshold: f64,
    ) -> Option<(usize, usize, f64)> {
        let mut best = None;
        let mut best_val = min_threshold;
        // Sample in 8 directions at distances 2, 4, ..., radius
        for &(dx, dy) in &EIGHT_DIRS {
            let mut dist = 2;
            while dist <= radius {
                let sx = cx as isize + dx * dist as isize;
                let sy = cy as isize + dy * dist as isize;
                if sx >= 0 && sy >= 0 {
                    let val = self.get(sx as usize, sy as usize);
                    if val > best_val {
                        best_val = val;
                        best = Some((sx as usize, sy as usize, val));
                    }
                }
                dist += 2;
            }
        }
        best
    }
}
```

Sampling 8 directions at staggered distances (not scanning every tile in the radius) keeps the cost at O(8 * radius/2) = O(4 * radius) per query, which is ~32 lookups for radius 8. Negligible.

### Integration with Existing TrafficMap

The existing `TrafficMap` in `src/simulation.rs` has the exact right shape for `TraceLayer`. Migration path:

1. Rename `TrafficMap` to `TraceLayer` (or keep `TrafficMap` as a type alias).
2. Add `decay_rate` and `decay_interval` fields to `TraceLayer`.
3. Move `road_candidates` to a standalone function that takes `&TraceLayer` and `&TileMap`.
4. Replace `Game::traffic: TrafficMap` with `Game::traces: EnvironmentalTraces`.
5. The existing `update_traffic()` in `game/build.rs` becomes `update_traces()`, calling emission for all layers.

The `foot_traffic` layer preserves ALL existing behavior: `step_on` in the movement loop, `decay` every 10 ticks, `road_candidates` every 100 ticks. The only addition is the A* cost discount, which is new behavior layered on top.

### Visual Representation (Pillar 4)

Traces should be visible through the overlay system (already has overlay cycling via `o` key):

**Traffic Overlay (exists conceptually, now enhanced):**
- Low traffic: faint footprints on grass tiles (character change: `.` to `,`)
- Medium traffic: visible dirt path (character change: `,` to `:`)
- High traffic: worn trail, pre-road (character change: `:` to `=`)
- Above threshold: auto-converts to Road (existing behavior)

**Danger Overlay:**
- Red tint intensity proportional to danger scent
- Skull marker (`!`) at tiles above danger threshold
- Visible in both Map and Landscape rendering modes

**Gather Scent Overlay:**
- Green/yellow tint around active gathering areas
- Shows the player where villagers are concentrating effort

**Territory Overlay:**
- Blue tint around owned buildings, fading at edges
- Shows the effective settlement boundary as perceived by AI

In Landscape mode (Mode B from game_design.md), traces modify terrain rendering subtly: high-traffic grass gets browner, danger zones get a reddish cast, territory zones get slightly warmer lighting. The world visually carries its history without needing an explicit overlay toggle.

## Performance Budget

| Component | Per tick (30 villagers) | Per tick (500 villagers) |
|-----------|----------------------|------------------------|
| Emission (6 layers, one write per villager per relevant layer) | ~3 us | ~50 us |
| Diffusion (6 layers, 256x256, every 10 ticks) | ~0 (off-tick) / ~600 us (on-tick) | same (map-bound, not pop-bound) |
| Decay (6 layers, 256x256, staggered intervals) | ~0 (off-tick) / ~200 us (on-tick) | same |
| AI reads (sample_gradient, ~32 lookups per query, ~2 queries per villager) | ~4 us | ~65 us |
| A* cost modifier (1 lookup per expanded node) | ~1 us per path | same per path |
| **Amortized total per tick** | **~90 us** | **~200 us** |

Diffusion is the expensive operation but runs infrequently (every 10 ticks) and is map-sized, not population-sized. At 256x256 = 65K tiles, a blur pass is ~100 us per layer. Six layers at staggered intervals means at most 2-3 layers diffuse on any given tick.

Storage: 6 layers * 256 * 256 * 8 bytes = ~3 MB. Acceptable.

## Migration Path

### Step 1: Generalize TrafficMap into TraceLayer

- Extract `TraceLayer` from the existing `TrafficMap` code in `src/simulation.rs`
- Add `decay_rate`, `decay_interval`, `diffuse()`, and `sample_gradient()` methods
- `TrafficMap` becomes a type alias or thin wrapper for backward compatibility
- `EnvironmentalTraces` struct bundles all six layers
- Replace `Game::traffic` with `Game::traces`
- Existing tests pass unchanged (foot_traffic layer has identical behavior)
- **Test:** `TraceLayer` unit tests for emission, decay, diffusion, gradient sampling

### Step 2: Emit gather scent and danger scent

- In `system_hunger` / resource pickup code: emit gather scent at pickup location
- In `system_ai_villager` flee transition: emit danger scent at flee location
- In `system_death`: emit strong danger scent at death location
- No AI reads yet — just accumulating data
- **Test:** after 500 ticks of normal gameplay, gather_scent is concentrated near resource sites, danger_scent appears where wolf attacks occurred

### Step 3: AI reads foot traffic in pathfinding

- Modify A* cost function to apply traffic discount
- Verify that villagers start preferring worn paths
- **Test:** two equal-length paths to a resource, one with prior traffic — villagers converge on the trafficked path

### Step 4: AI reads gather scent and danger scent

- Explorers use foot traffic novelty bonus
- Resource-seekers follow gather scent gradient when personal memory is empty
- Pathfinding adds danger scent penalty
- **Test:** new villager with no memory finds resource area by following gather scent. Villagers route around recent wolf attack site.

### Step 5: Home scent, depletion marks, territory

- Buildings emit home scent; migrants follow it to find settlement
- Failed gathering emits depletion marks; subsequent villagers avoid marked sites
- Idle villagers near buildings emit territory marks
- **Test:** migrant spawned at map edge navigates to settlement via home scent gradient. Depleted forest accumulates depletion marks, subsequent gatherers prefer un-marked forests.

### Step 6: Visual overlays

- Add trace overlay modes to the existing overlay cycle
- Landscape mode picks up subtle trace-driven tinting
- **Test:** visual inspection — worn paths visible, danger zones red-tinted, territory boundary visible

## Edge Cases

**New game (tick 0):** All trace layers are zero. No information in the world. Villagers rely entirely on sight and memory. Traces build up over the first few hundred ticks as villagers begin moving and gathering. The world starts blank and gradually fills with history.

**Abandoned area:** If villagers stop visiting an area, all traces decay to zero. The path fades, gather scent vanishes, territory shrinks. The area returns to wilderness. If villagers return later, they must re-establish traces from scratch. This is intentional: abandoned outposts are forgotten by the world.

**Conflicting signals:** A tile can have both high gather scent AND high danger scent (resource site near wolf territory). The AI handles this naturally: gather scent attracts, danger scent repels through pathfinding cost. A desperate villager (high hunger, no alternatives) will still go there — the danger penalty is a cost increase, not a hard block.

**Map edges:** `TraceLayer::get()` returns 0.0 for out-of-bounds coordinates (same as `TrafficMap`). Diffusion skips boundary tiles. Traces don't wrap.

**Save/load:** All trace layers serialize via `serde` (Vec<f64>). Round-trip tested. Trace state is part of the saved game — loading a save at tick 5000 should show 5000 ticks of accumulated history in the trace layers.

**Multiple settlements:** Each trace layer is global (covers the whole map). If future multi-settlement support lands, traces from different settlements naturally overlap and compete. Territory marks from two settlements create a visible contested border zone. No per-settlement trace layers needed.

## Interaction with Other Systems

**Per-Villager Memory (per_villager_memory.md):** Traces and memory are complementary, not redundant. Memory is personal and precise ("I saw stone at (50, 20)"). Traces are communal and fuzzy ("something gatherable is vaguely northeast"). Memory drives targeted behavior; traces drive ambient/fallback behavior. A villager with good memory ignores traces. A villager with empty memory relies on them.

**Stockpile Bulletin Board (stockpile_bulletin_board.md):** The bulletin board shares explicit reports ("stone at (50, 20)"). Traces share implicit signals (gather scent gradient pointing northeast). The bulletin board requires visiting the stockpile; traces are readable anywhere. They serve different scales: bulletin board for cross-map knowledge, traces for local navigation.

**Road Auto-Build (existing):** The foot_traffic trace layer IS the road-building mechanism. Environmental traces generalize it. The `road_candidates` threshold remains the crystallization point where a soft trace becomes hard infrastructure. Roads are the only trace that permanently alters terrain.

**Threat Scaling (threat_scaling.md):** Territory marks give the threat system a spatial signal. Instead of scaling threats purely on population count, wolf AI could be attracted to areas with LOW territory marks (undefended frontier) and repelled by HIGH territory marks (well-patrolled core). Threats naturally probe the settlement's weak points.

## Observable Behavior Changes (Pillar 4)

What the player should notice:

1. **Paths form before roads.** Grass between the stockpile and the forest gets subtly worn. Then it becomes a road. The progression is visible.
2. **New villagers aren't lost.** A migrant arriving at the map edge drifts toward the settlement by following home scent, not by omniscient pathing.
3. **Danger zones are avoided.** After a wolf attack, villagers route around the area for a while. The detour is visible. Gradually they resume the direct path.
4. **Gathering concentrates visibly.** The gather scent overlay shows hotspots where resource extraction is happening. A depleted hotspot fades as villagers shift to new sources.
5. **Territory has a shape.** The territory overlay shows the settlement's effective footprint — not a circle, but an organic shape following buildings and patrol patterns.
6. **The map tells a story.** At tick 10000, the trace overlays show the settlement's entire history: where it gathered, where it fought, where it expanded, where it retreated. The world is a palimpsest.
