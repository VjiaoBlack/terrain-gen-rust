# Resource Flow Visibility

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 4 (Observable Simulation), 1 (Geography Shapes Everything)*

## Problem

The settlement has a logistics network -- villagers haul wood from forests, stone from deposits, food from farms -- but you cannot see it. A hauling villager looks identical to an idle one (both render as `V`). The paths between stockpiles and resource sites are invisible until they hit the 300.0 traffic threshold and convert to roads. The player cannot answer "where are my supply lines?" or "which resource route is busiest?" without activating the Traffic overlay, which shows raw heat values rather than legible flow.

The game design doc states: *"Visible resource flow (gatherers walk to resources, carry them back, deposit visibly)"* as a Phase 1 deliverable. Pillar 4's success criterion requires a new player to narrate what's happening in under 30 seconds. Currently, resource logistics -- the heartbeat of the settlement -- is silent.

The entity_state_visibility doc solves half the problem: hauling villagers get directional arrows colored by cargo type. This doc addresses the other half: making the *network itself* readable -- the worn paths, the flow direction, the supply line structure.

## Design Goals

1. **Supply lines are visible without overlays.** In normal play, frequently-traveled paths between stockpile and resource sites show wear. You can see the logistics skeleton of your settlement.
2. **Haulers are visually distinct.** A villager carrying stone looks different from one carrying wood, and both look different from an idle villager. (Builds on entity_state_visibility.md.)
3. **Flow direction is readable.** You can tell which way resources move along a path -- toward the stockpile or toward the gathering site.
4. **The overlay adds detail, not basics.** The Traffic overlay becomes a "supply line analysis" mode showing throughput, bottlenecks, and resource type per route -- not the only way to see that logistics exist.
5. **Worn paths tell history.** A path that was once busy but is now abandoned should fade visibly, telling the story of a depleted resource or a shifted supply route.

## Current State

### TrafficMap (`src/simulation.rs`, line 982)

The `TrafficMap` struct tracks accumulated foot traffic per tile as `f64` values:
- `step_on(x, y)` increments by 1.0 each time a villager stands on a tile.
- `decay()` multiplies all values by 0.999 per call (called every 10 ticks in `update_traffic()`).
- `road_candidates()` returns tiles above a threshold (currently `ROAD_TRAFFIC_THRESHOLD = 300.0`) for auto-conversion to `Terrain::Road`.

### Traffic Overlay (`src/game/render.rs`, line 1440)

The existing Traffic overlay renders tiles with traffic > 1.0 as:
- `'·'` (dot) for sub-threshold traffic, colored dim yellow to orange based on intensity.
- `'='` for tiles at or above the road threshold, colored bright orange.
- Background: `Color(40, 30, 5)` dark amber.

This shows where traffic exists but not what type of resource flows there or which direction.

### Hauling State (`src/ecs/components.rs`, line 126)

`BehaviorState::Hauling { target_x, target_y, resource_type }` stores the stockpile destination and what the villager carries. The `resource_type` field already distinguishes wood, stone, food, grain, planks, and masonry -- all the information needed for visual differentiation.

### Road Auto-Build (`src/game/build.rs`, line 338)

`update_traffic()` records villager positions every tick, decays every 10 ticks, and converts candidates to `Terrain::Road` every 100 ticks. Roads grant 1.5x movement speed, creating a positive feedback loop: more traffic creates roads, roads make the route faster, faster routes attract more haulers.

## Proposed Design

### Layer 1: Worn Terrain (always visible, no overlay)

Traffic below the road conversion threshold should still produce visible terrain changes. The landscape tells the story of where villagers walk.

#### Traffic Tiers and Visual Representation

| Traffic Value | Tier | Map Mode | Landscape Mode | Meaning |
|---------------|------|----------|----------------|---------|
| 0 - 10 | None | Normal terrain | Normal terrain | Untouched |
| 10 - 50 | Faint | Terrain char dims slightly | Slightly desaturated ground color | Occasional foot traffic |
| 50 - 150 | Worn | `·` replaces grass char, terrain color shifts to tan `(160, 140, 100)` | Ground texture thins, brownish tint blends with terrain | Regular foot traffic, visible path forming |
| 150 - 300 | Trail | `-` or `─` oriented along dominant direction, tan-brown `(140, 110, 70)` | Clear brown line, reduced vegetation texture | Well-established trail, not yet a road |
| 300+ | Road | Auto-converts to `Terrain::Road` (existing behavior) | Full road rendering | Permanent infrastructure |

**Key detail: Worn and Trail tiers render in the base terrain pass, not as an overlay.** This means supply lines are visible during normal play. The terrain itself changes appearance from foot traffic -- you see paths emerge organically, exactly as described in the game design doc's anti-goal: *"Roads emerge from traffic patterns."*

#### Directional Wear

For Trail-tier paths (150-300), the character should reflect the dominant direction of travel across the tile. Track this by extending `TrafficMap` with a directional component:

```
// In TrafficMap, add per-tile direction accumulator
traffic_dx: Vec<f64>,  // sum of movement dx across tile
traffic_dy: Vec<f64>,  // sum of movement dy across tile
```

When a villager steps on a tile, record not just the step but the direction they were moving. The accumulated vector determines trail orientation:

| Dominant Direction | Trail Char (Map Mode) |
|--------------------|-----------------------|
| Horizontal (east-west) | `─` or `-` |
| Vertical (north-south) | `│` or `|` |
| Diagonal NE-SW | `/` |
| Diagonal NW-SE | `\` |
| Mixed / no dominant | `·` |

This creates trails that visually *connect* resource sites to stockpiles, forming readable lines on the map.

### Layer 2: Hauler Appearance (always visible)

This builds directly on entity_state_visibility.md's hauling entries. The key additions specific to resource flow readability:

#### Haulers Are Brighter Than Seekers

A villager walking toward a resource site (Seek) is dimmer than a villager walking back with cargo (Hauling). This creates a visual asymmetry: the return trip is more prominent. You see bright arrows flowing toward the stockpile and dimmer ones flowing away. The aggregate effect at distance: **you can see which direction resources flow.**

| State | Brightness | Rationale |
|-------|------------|-----------|
| Seek(resource site) | 70% | Outbound, empty-handed |
| Hauling(resource) | 100% | Inbound, carrying cargo |

#### Cargo Glyph Suffix (Map Mode, optional future)

In Map mode, hauling villagers could render as a two-character sequence when the terminal cell is wide enough: the directional arrow plus a cargo indicator. For standard single-cell rendering, the arrow color already encodes resource type (per entity_state_visibility.md):

| Cargo | Arrow Color | Mnemonic |
|-------|-------------|----------|
| Wood | `(180, 120, 50)` warm brown | Tree bark |
| Stone | `(180, 180, 180)` light grey | Rock |
| Food | `(100, 220, 80)` bright green | Plants |
| Grain | `(220, 200, 80)` wheat gold | Wheat field |
| Planks | `(200, 160, 80)` tan | Processed wood |
| Masonry | `(200, 200, 210)` off-white | Cut stone |

#### Hauler Trail Particles (Landscape Mode)

In Landscape mode, hauling villagers leave 1-2 fading particle dots behind them in their cargo color. This creates short colored streaks showing:
- Direction of travel (trail behind the hauler).
- Resource type (trail color).
- At scale, rivers of color flowing toward the stockpile.

### Layer 3: Supply Line Overlay (enhanced Traffic overlay)

The existing Traffic overlay (`OverlayMode::Traffic`) is upgraded from a simple heat map to a supply line analysis view.

#### Resource-Typed Flow

Extend `TrafficMap` to track resource type per tile:

```
// Per-tile resource flow counters
flow_by_type: Vec<[f64; ResourceType::COUNT]>,
```

Increment the counter for the hauler's resource type when a hauling villager steps on a tile. This allows the overlay to color paths by dominant resource:

| Dominant Resource | Overlay Color |
|-------------------|---------------|
| Wood | `(160, 100, 40)` brown |
| Stone | `(160, 160, 170)` grey |
| Food | `(60, 180, 60)` green |
| Grain | `(200, 180, 60)` gold |
| Planks | `(180, 140, 60)` tan |
| Masonry | `(180, 180, 200)` off-white |
| Mixed (no dominant) | `(200, 150, 50)` amber (current) |

#### Flow Direction Arrows

Every N tiles along a high-traffic path (traffic > 50), render a directional arrow based on the accumulated `traffic_dx`/`traffic_dy` for that tile. The arrows point in the net flow direction. Since haulers go outbound empty and inbound loaded, and we weight hauling steps more heavily in the directional accumulator, arrows naturally point toward the stockpile -- showing where resources flow TO.

#### Throughput Width

In the overlay, path "width" encodes throughput:
- Low traffic (10-50): single-tile thin line.
- Medium traffic (50-150): renders on the tile plus a dim tint on adjacent tiles.
- High traffic (150+): full 3-tile-wide rendering.

This makes the busiest supply lines visually thicker, creating a vascular system appearance when zoomed out -- arteries to the stockpile, capillaries to scattered resource sites.

#### Bottleneck Highlighting

Where multiple supply lines converge into a single tile (e.g., a bridge or narrow pass), the overlay flashes the tile in a distinct color (`(255, 100, 50)` warning orange). This helps the player identify where a road, bridge, or new stockpile would have the most impact.

### Layer 4: Stockpile Pulse (future)

When resources are deposited at a stockpile, a brief radial pulse of the resource's color emanates 1-2 tiles outward. At a busy stockpile, you see a steady rhythm of colored pulses -- a visual heartbeat showing the logistics network is alive and delivering.

## Implementation Plan

### Phase 1: Worn Terrain Rendering

Modify the terrain rendering pass in `render.rs` to check `TrafficMap` values for each visible tile. For traffic in the 10-300 range, blend the terrain's character and color toward the worn-path appearance defined in the tier table above.

Key changes:
- `src/game/render.rs`: In the terrain draw loop, after computing terrain char/color, check `self.traffic.get(wx, wy)` and apply worn-path visual if above the faint threshold.
- No simulation changes needed -- the existing `TrafficMap` values drive this directly.

### Phase 2: Directional Traffic Tracking

Extend `TrafficMap` in `src/simulation.rs` with `traffic_dx` and `traffic_dy` vectors. Modify `step_on()` to accept a direction parameter:

```rust
pub fn step_on_directed(&mut self, x: usize, y: usize, dx: f64, dy: f64) {
    if x < self.width && y < self.height {
        let idx = y * self.width + x;
        self.traffic[idx] += 1.0;
        self.traffic_dx[idx] += dx;
        self.traffic_dy[idx] += dy;
    }
}
```

Update `update_traffic()` in `src/game/build.rs` to pass villager velocity when recording steps. The existing `step_on()` remains as a non-directed fallback.

Apply direction-dependent trail characters at the Trail tier in the terrain rendering.

### Phase 3: Resource-Typed Flow Tracking

Add `flow_by_type` array to `TrafficMap`. When a villager in `BehaviorState::Hauling` steps on a tile, increment the counter for their `resource_type`. Apply a weight multiplier (2.0x) for hauling steps in the directional accumulator so net flow direction points toward stockpiles.

Upgrade the Traffic overlay to use `flow_by_type` for color and `traffic_dx`/`traffic_dy` for direction arrows.

### Phase 4: Hauler Particles and Stockpile Pulse

Requires the particle system (already exists in `render.rs`). Hauling villagers emit trailing particles. Stockpile deposits trigger a brief pulse. Both are Landscape mode only.

## Data Changes

### TrafficMap Extensions

```rust
pub struct TrafficMap {
    pub width: usize,
    pub height: usize,
    traffic: Vec<f64>,
    traffic_dx: Vec<f64>,          // Phase 2
    traffic_dy: Vec<f64>,          // Phase 2
    flow_by_type: Vec<[f64; 6]>,   // Phase 3: indexed by ResourceType
}
```

Memory cost for a 256x256 map:
- Existing `traffic`: 512 KB
- New `traffic_dx` + `traffic_dy`: 1 MB
- New `flow_by_type` (6 resource types): 3 MB
- Total: ~4.5 MB -- acceptable for a single map.

Decay applies to all vectors uniformly. Serialization extends the existing `TrafficMap` serde implementation.

## Testing

- **Visual regression**: Run `cargo run --release -- --play --ticks 2000` and verify worn paths appear between stockpile and resource sites.
- **Unit tests for TrafficMap extensions**: Test `step_on_directed()` accumulates direction correctly. Test that dominant direction computation returns expected results for known inputs.
- **Unit tests for worn terrain rendering**: Given a known traffic value, assert the correct character and color are produced.
- **Flow direction test**: Place a stockpile at (10, 10) and a resource at (20, 10). Simulate 100 hauling round-trips. Assert `traffic_dx` along the path is net-negative (pointing toward stockpile at lower x). Assert overlay arrows point left.
- **Glance test**: Show a mid-game frame to someone unfamiliar. Ask "where are the supply lines?" Target: they can trace at least one route from resource to stockpile without prompting.

## Interaction with Existing Systems

- **entity_state_visibility.md**: This doc extends that design. Hauler arrow colors and direction indicators from that doc are prerequisites. This doc adds the network-level visibility (worn paths, typed flow overlay) that makes individual hauler appearance part of a larger readable system.
- **Traffic overlay**: The existing overlay is upgraded, not replaced. Current heat-map behavior becomes the fallback for tiles without typed flow data.
- **Road auto-build**: Unchanged. Worn terrain at the Trail tier (150-300) acts as a visual preview of where roads will eventually form. The player can see roads "growing" from faint paths to trails to permanent roads.
- **Exploration map**: Worn paths only render on revealed tiles (`exploration.is_revealed()`), consistent with fog-of-war.
- **Save/load**: `TrafficMap` already serializes via serde. The new fields must be added with `#[serde(default)]` for backward compatibility with existing save files.

## Open Questions

1. **Should worn paths affect pathfinding before becoming roads?** A trail-tier path (150-300) could grant a small speed bonus (1.1x-1.2x) to create a smoother ramp toward the road threshold. Risk: adds complexity to the terrain speed table.
2. **Overlay keybinding.** Should the supply line overlay be a sub-mode of the Traffic overlay (press `o` to cycle to Traffic, press again to cycle between heat/flow/bottleneck views)? Or a separate overlay mode?
3. **Performance at scale.** Checking `TrafficMap` for every visible tile in the terrain pass adds one f64 lookup per tile per frame. For a 120x40 viewport, that is 4800 lookups -- negligible. The `flow_by_type` array is larger but only accessed in overlay mode.
4. **Worn paths on non-grass terrain.** Should sand, forest, and snow also show wear? Forest wear could mean undergrowth is trampled (lighter green). Snow wear could mean packed snow (slightly different shade). Sand may not show wear realistically. Decision: start with grass only, extend to other terrain types based on visual testing.
5. **Particle budget.** If 50 haulers each emit 2 trailing particles, that is 100 extra particles per frame. Acceptable given the existing particle system, but should be benchmarked at 500+ villagers.
