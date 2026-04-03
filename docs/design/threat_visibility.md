# Threat Visibility

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 4 (Observable Simulation), 1 (Geography Shapes Everything)*

## Problem

The player cannot see danger before it arrives. Wolves appear at the forest edge and immediately attack. Raider events resolve instantly with no visual approach. The Threats overlay (`OverlayMode::Threats`) shows wolf and den positions with an 8-tile danger radius and garrison locations in green, but it provides no information about *where threats will come from*, *what territory predators claim*, or *where defenses are actually covering*. The player is reactive, never proactive.

The game design doc (Pillar 4, Rich tier) calls for: "wolf territory shown as subtle color shift, approaching pack visible from distance." The threat_scaling design doc introduces geographic spawn points (forests for wolves, corridors for raiders) and chokepoint detection, but has no visual specification for how these systems communicate threat information to the player.

Today's Threats overlay is a simple presence indicator: red dots where wolves are, green dots where garrisons are. It answers "where are the threats right now?" but not:

- Where do threats *come from*?
- Where is my settlement *exposed*?
- Where are my defenses *actually covering*?
- Is a pack forming at the forest edge?

Without this information, the player cannot make strategic defense decisions. Garrison placement is guesswork.

## Design Goals

1. **Threat sources are visible before attacks launch.** Wolf territory and raider approach corridors are shown as persistent zones on the map, not just during active attacks.
2. **Approaching threats are visible at distance.** A wolf pack forming at the forest edge or raiders marching down a road are visible many ticks before they arrive.
3. **Garrison coverage is spatially explicit.** The player can see exactly which approach corridors each garrison covers and where gaps exist.
4. **Both Map and Landscape modes communicate threat information** through their respective visual languages (glyphs/flat color vs. color fields/atmosphere).
5. **Information layers compose cleanly.** Threat visibility integrates with the existing overlay system without creating visual noise in normal play.

## Current State

### Threats Overlay (`draw_threat_overlay` in `render.rs`)

The current implementation draws three layers:

| Layer | Glyph | Color | Logic |
|-------|-------|-------|-------|
| Danger zone | `·` | fg `(180, 40, 40)` bg `(60, 10, 10)` | 8-tile radius circle around each `Den` entity |
| Wolves | `W` | fg `(255, 50, 50)` bg `(80, 0, 0)` | Every `Creature` with `Species::Predator` |
| Dens | `D` | fg `(255, 80, 80)` bg `(80, 0, 0)` | Every `Den` entity |
| Garrisons | `G` | fg `(50, 255, 50)` bg none | Every `GarrisonBuilding` entity |
| Town Halls | `H` | fg `(255, 220, 60)` bg none | Every `TownHallBuilding` entity |

Problems:
- Danger zones are circular blobs around dens, not wolf territory. Wolves that wander far from dens have no associated danger zone.
- No directional information. The player cannot tell which direction the next attack will come from.
- Garrison coverage is a single tile marker. No indication of defensive radius or which approaches it covers.
- No distinction between "quiet forest with wolves" and "pack actively forming to attack."
- No chokepoint or corridor visualization (even though threat_scaling.md designs these systems).

### Territory Overlay (`OverlayMode::Territory`)

The Territory overlay tints tiles by `InfluenceMap` value with a blue wash (alpha up to 0.3). This shows where the settlement's influence reaches. Threat visibility should show the *complement*: where influence is weak and threats are strong.

## Design

### Layer 1: Wolf Territory Zones

Wolf territory replaces the current 8-tile den radius with a geographically accurate zone derived from forest clusters (as designed in threat_scaling.md).

**Data source:** `find_forest_clusters()` produces a list of forest clusters with tile membership, centroid, and size. Each qualifying cluster (within 15-60 tiles of settlement, size > 20 tiles) is a potential wolf territory.

**Computation:**
```
wolf_territory[tile] = 1.0 if tile is Forest AND belongs to a qualifying cluster
                       0.5 if tile is within 3 tiles of a qualifying cluster edge (buffer zone)
                       0.0 otherwise
```

The buffer zone represents the "edge of the woods" -- the transitional area where wolves lurk before attacking. This is where the player should be watching.

**Threat intensity modifier:** Each cluster's visual intensity scales with the threat tier (from threat_scaling.md):

```
cluster_intensity = base_intensity * (1.0 + tier * 0.2)
```

At Tier 0 (Quiet), wolf territory is barely visible. At Tier 3+ (Wealthy), it pulses with menace.

#### Map Mode Rendering

Wolf territory tiles get a background tint that shifts the base terrain color toward dark red-brown. The forest glyph (`♠`) remains but the background communicates danger.

| Zone | Background Tint | Glyph | Effect |
|------|----------------|-------|--------|
| Core territory (forest tile in cluster) | Shift bg toward `(60, 15, 15)` at alpha 0.15-0.30 | Keep terrain glyph | Subtle darkening of forest, reads as "deep woods" |
| Buffer zone (3-tile edge) | Shift bg toward `(80, 25, 10)` at alpha 0.10-0.20 | Keep terrain glyph | Slight warmth at forest edge, transitional |
| Active pack staging | Shift bg toward `(120, 30, 20)` at alpha 0.25 | Keep terrain glyph | Noticeably redder when wolves are gathering |

The tint is applied by blending with the existing background color:
```
tinted_bg = Color(
    (base_bg.r * (1.0 - alpha) + tint.r * alpha) as u8,
    (base_bg.g * (1.0 - alpha) + tint.g * alpha) as u8,
    (base_bg.b * (1.0 - alpha) + tint.b * alpha) as u8,
)
```

This matches the approach already used by the Territory overlay for influence tinting.

#### Landscape Mode Rendering

In Landscape mode, wolf territory manifests as an atmospheric color shift across the forest region. The lighting system already applies ambient tint; wolf territory adds a secondary tint layer.

| Zone | Color Shift | Texture Effect |
|------|------------|----------------|
| Core territory | Desaturate greens, push toward olive-brown `(50, 45, 20)` | Denser texture characters (`♣"` become more frequent) |
| Buffer zone | Slight desaturation at forest edge | Normal texture density |
| Active staging | Warm red undertone, like firelight from within the forest | Occasional `·` flicker character (eyes in the dark) |

The key principle: in Landscape mode, the forest itself looks *different* where wolves live. Darker, denser, less inviting. A player scanning the horizon reads "that forest looks threatening" without needing to check an overlay.

### Layer 2: Approach Corridor Visualization

Raider approach corridors (from threat_scaling.md's `scan_approach_corridors()`) are rendered as directional indicators showing where the settlement is exposed.

**Data source:** `scan_approach_corridors()` returns corridor structs with direction, width, and walkability score. Corridors are categorized:

| Corridor Type | Width | Weight | Visual Priority |
|---------------|-------|--------|----------------|
| Mountain pass | 1-6 tiles | 3x (highest threat) | High -- always shown |
| Narrow approach | 7-12 tiles | 1.5x | Medium -- shown when Threats overlay active |
| Open plains | 13+ tiles | 1.0x | Low -- shown only as directional arrow |

**Computation:** Corridors are precomputed at world-gen and cached. Recomputed if terrain changes (building placement, deforestation) within 10 tiles of a corridor.

#### Map Mode Rendering

Corridors render as directional markers along the approach path:

| Element | Glyph | Color | Placement |
|---------|-------|-------|-----------|
| Approach direction arrow | `←` `→` `↑` `↓` `↗` `↘` `↖` `↙` | `(200, 100, 40)` amber | Every 8 tiles along corridor centerline, from map edge toward settlement |
| Mountain pass marker | `╬` | `(220, 80, 40)` bright amber | At the narrowest point of the pass |
| Corridor edge (impassable flanks) | Existing mountain/cliff glyph | Tinted `(140, 60, 30)` warm brown | On impassable tiles bordering the corridor |

Arrows point *toward the settlement*, showing the player which direction threats approach from. The arrows are sparse (every 8 tiles) to avoid clutter.

#### Landscape Mode Rendering

In Landscape mode, corridors are shown as subtle ground coloring rather than arrows:

| Element | Visual Treatment |
|---------|-----------------|
| Corridor floor | Slight warm tint on ground color, `(+15, +5, -5)` RGB offset. Suggests worn/traveled ground. |
| Mountain pass | Brighter warm tint, narrow band of lighter ground between dark mountain walls |
| Open approach | Very subtle -- only visible as absence of mountain/forest barrier |

The effect: mountain passes glow faintly warm between dark cliff walls. The player's eye is naturally drawn to them as openings in the terrain.

### Layer 3: Garrison Coverage Zones

Each garrison building projects a defensive coverage area. This replaces the single green `G` marker with a spatial zone showing what the garrison actually protects.

**Computation:**

```
garrison_coverage_radius = 12 tiles (base)
garrison_effective_radius = garrison_coverage_radius + chokepoint_bonus

// Coverage is directional: stronger toward approach corridors
coverage[tile] = garrison.defense_bonus / distance_to_garrison
                 * corridor_alignment_factor
```

Where `corridor_alignment_factor` is 1.5 if the tile lies along a detected approach corridor and 1.0 otherwise. Garrisons at chokepoints get an additional 5-tile radius bonus (matching the chokepoint_bonus from threat_scaling.md).

**Coverage overlap:** Where two garrisons' coverage zones overlap, the coverage values add. This visually rewards the player for creating overlapping fields of fire.

#### Map Mode Rendering

Coverage zones use a green background tint, complementing the red threat tinting:

| Coverage Level | Background Tint | Meaning |
|---------------|----------------|---------|
| Strong (coverage > 0.6) | Shift toward `(20, 80, 30)` at alpha 0.20 | Well-defended, garrison is nearby |
| Moderate (0.3-0.6) | Shift toward `(20, 80, 30)` at alpha 0.12 | Partially covered, garrison can respond |
| Weak (0.1-0.3) | Shift toward `(20, 60, 30)` at alpha 0.06 | Edge of coverage, garrison might not reach in time |

The garrison building itself still renders as `G` in bright green `(50, 255, 50)`. Chokepoint-placed garrisons get a `+` suffix or brighter glow to indicate the positioning bonus.

#### Landscape Mode Rendering

Garrison coverage manifests as a subtle sense of safety -- slightly warmer, slightly brighter tones in the area around the garrison.

| Coverage Level | Visual Treatment |
|---------------|-----------------|
| Strong | Ground is slightly greener/warmer. Torch-like warmth near the garrison building itself. |
| Moderate | Faint green-gold undertone, barely perceptible unless compared to uncovered terrain. |
| Weak | No visible effect. The absence of coverage reads as "exposed" in contrast to covered areas. |

### Layer 4: Active Threat Approach Indicators

When a wolf pack or raider party has actually spawned and is approaching, the visualization escalates from ambient zones to active threat tracking.

**Wolf pack approach:**

| Stage | Map Mode | Landscape Mode | Duration |
|-------|----------|---------------|----------|
| Staging (lurking at forest edge) | `W` glyphs appear at forest edge, dim red `(160, 50, 50)`. Pulsing bg on forest-edge tiles. | Red-amber flicker at forest edge, like eyes catching firelight | 30-50 ticks (per threat_scaling.md linger period) |
| Advancing | `W` glyphs move toward settlement, bright red `(255, 50, 50)`. Trail of `·` dots in dim red behind them. | Bright red points moving against dark terrain. Motion is the signal. | Until arrival |
| Engaged | Existing combat rendering (wolves near villagers) | Same, with emphasis on contrast against terrain | Until resolved |

**Raider approach:**

| Stage | Map Mode | Landscape Mode | Duration |
|-------|----------|---------------|----------|
| Spotted (40-60 tiles out) | `!` warning marker at detection point, amber `(220, 160, 40)` | Bright amber point at distance, reads as campfire/torchlight | 1 tick (notification) |
| Marching | `B` glyphs moving along corridor/road, amber `(200, 120, 40)`. Arrow glyphs along their path pulse brighter. | Amber points moving along the warm-tinted corridor | Until arrival |
| At gates | `B` glyphs near stockpile, bright red `(255, 80, 40)` | Red-amber cluster at settlement edge | Until resolved |

**Event log integration:** Directional information in the event log reinforces the visual. Examples:
- "Wolf pack gathering at the northern forest edge!" (when staging begins)
- "Raiders approaching from the mountain pass to the southwest!" (when spotted)
- "Threat repelled -- garrison at the eastern chokepoint held!" (on successful defense)

### Layer 5: Exposure Gaps

The most actionable information: where is the settlement exposed? This is computed as the difference between threat sources and garrison coverage.

**Computation:**
```
exposure[tile] = threat_pressure[tile] - garrison_coverage[tile]

threat_pressure[tile] = sum of:
  - wolf_territory proximity (0.0-1.0, based on distance to nearest qualifying cluster)
  - corridor_exposure (0.0-1.0, tiles along approach corridors with no garrison coverage)
```

Tiles with `exposure > 0.3` are "gaps" -- places where threats can approach without garrison resistance.

#### Map Mode Rendering

Exposure gaps render as a pulsing amber border around the settlement's influence zone, specifically at points where approach corridors intersect the territory boundary without garrison coverage.

| Element | Glyph | Color | Placement |
|---------|-------|-------|-----------|
| Exposure gap marker | `!` | `(220, 160, 40)` amber, pulsing (alternate bright/dim every 30 ticks) | At the intersection of approach corridor and settlement influence boundary |
| Suggested garrison site | `?` | `(100, 200, 100)` soft green | At detected chokepoints within 20 tiles of settlement that lack a garrison |

The `?` markers are particularly important: they show the player where a garrison would be most effective. This connects the threat visualization to actionable building decisions without violating the "no micromanagement" anti-goal -- the player still chooses whether and when to build, but the map tells them *where*.

#### Landscape Mode Rendering

Exposure gaps are visible as the contrast between covered and uncovered territory edges. Where garrison coverage fades and wolf territory begins, the terrain transitions from warm safe tones to cold threatening tones with no intermediate buffer. The visual "cliff" in color temperature signals danger.

No explicit markers in Landscape mode. The information is environmental.

## Overlay Integration

### Threats Overlay (`OverlayMode::Threats`)

The Threats overlay becomes the primary view for all five layers. When active, it composites:

1. Wolf territory zones (red-brown forest tint)
2. Approach corridor arrows (amber directional markers)
3. Garrison coverage zones (green tint)
4. Active threat indicators (moving glyphs)
5. Exposure gap markers (`!` and `?`)

Rendering order matters -- later layers draw on top of earlier ones:
```
1. Base terrain (normal render)
2. Wolf territory tint (background blend)
3. Garrison coverage tint (background blend, composes with #2)
4. Corridor arrows (foreground glyphs, sparse)
5. Exposure markers (foreground glyphs, sparse)
6. Active threats (foreground glyphs, bright)
7. Garrison/Town Hall markers (foreground glyphs, bright)
```

### Normal Play (no overlay)

In normal play (`OverlayMode::None`), only Layer 4 (active threat approach) is visible. Staging wolves at the forest edge and marching raiders are rendered in both modes because they represent immediate, actionable information. The ambient territory and coverage zones require the overlay toggle to see -- this keeps normal play clean.

Exception: the `?` suggested garrison markers are also visible in normal play when the build menu is open and Garrison is selected. This gives the player placement guidance at the moment they need it.

### Other Overlays

- **Territory overlay:** Garrison coverage zones are also shown in Territory mode (green tint), since they represent the defended extent of the settlement. Wolf territory is NOT shown in Territory mode to avoid visual clutter.
- **Tasks overlay:** Active threat approach indicators (moving wolves/raiders) remain visible in Tasks mode, since fleeing villagers are color-coded there and the threat source provides context.

## Computation Details

### Wolf Territory Cache

Wolf territory zones are expensive to compute (flood-fill for forest clusters, distance checks). Cache and recompute on:

- World generation (initial computation)
- Deforestation events (tree removed within a qualifying cluster)
- Every 500 ticks (catch gradual changes)

Store as a `ThreatMap` parallel to `InfluenceMap`:
```rust
pub struct ThreatMap {
    pub width: usize,
    pub height: usize,
    wolf_territory: Vec<f32>,     // 0.0 = safe, 1.0 = core territory
    corridor_pressure: Vec<f32>,  // 0.0 = no corridor, 1.0 = primary approach
    garrison_coverage: Vec<f32>,  // 0.0 = uncovered, 1.0+ = well-covered
    exposure: Vec<f32>,           // computed: threat - coverage
}
```

Update `exposure` whenever `wolf_territory`, `corridor_pressure`, or `garrison_coverage` changes. The `exposure` field drives the gap markers.

### Garrison Coverage Computation

Garrison coverage updates when:
- A garrison is built or destroyed
- A chokepoint is detected or invalidated (terrain change)
- Every 100 ticks (accounts for wall construction changes)

Algorithm:
```
for each garrison G at position (gx, gy):
    for each tile (tx, ty) within garrison_coverage_radius:
        dist = sqrt((tx - gx)^2 + (ty - gy)^2)
        if dist > garrison_coverage_radius: continue
        
        // Line-of-sight check: coverage doesn't pass through mountains
        if !line_of_sight(G, tile): continue
        
        base_coverage = G.defense_bonus / (1.0 + dist * 0.15)
        
        // Bonus along approach corridors
        if tile_on_corridor(tx, ty):
            base_coverage *= 1.5
        
        // Chokepoint bonus
        if garrison_near_chokepoint(G):
            base_coverage *= 1.3
        
        garrison_coverage[ty * width + tx] += base_coverage
```

The line-of-sight check prevents garrisons from "covering" areas behind mountains. This makes garrison placement at passes more important -- a garrison behind a mountain covers the pass but not the plains on the other side.

### Approach Corridor Caching

Corridors are computed at world-gen and stored in the `Game` struct:

```rust
pub struct ApproachCorridor {
    pub direction: f64,          // radians from settlement center
    pub min_width: u32,          // narrowest point in tiles
    pub centerline: Vec<(i32, i32)>,  // tile positions along corridor center
    pub narrowest_point: (i32, i32),  // chokepoint position
    pub has_road: bool,
    pub threat_weight: f64,      // from threat_scaling scoring
}
```

Corridors are recomputed when:
- Settlement center moves significantly (>10 tiles)
- Major terrain change (building placed at corridor, deforestation)
- Every 1000 ticks (infrequent, corridors are mostly static)

## Implementation Plan

### Phase 1: ThreatMap Data Structure

- Add `ThreatMap` struct to `simulation.rs` alongside `InfluenceMap`.
- Add `threat_map: ThreatMap` field to `Game`.
- Implement `update_wolf_territory()` using forest cluster data (depends on threat_scaling Phase 2's `find_forest_clusters()`).
- Implement `update_garrison_coverage()` using garrison positions and simple distance falloff (no line-of-sight yet).
- Compute `exposure` as `wolf_territory + corridor_pressure - garrison_coverage`.

**Files:** `src/simulation.rs`, `src/game/mod.rs`

### Phase 2: Threats Overlay Rewrite

- Replace `draw_threat_overlay()` with layered rendering using `ThreatMap` data.
- Render wolf territory tint (Layer 1) as background blend on forest tiles.
- Render garrison coverage tint (Layer 3) as green background blend.
- Render exposure gap markers (Layer 5) as `!` and `?` glyphs.
- Keep existing wolf/den/garrison entity markers but render them on top of the new tint layers.

**Files:** `src/game/render.rs`

### Phase 3: Approach Corridor Visualization

- Depends on threat_scaling Phase 3 (`scan_approach_corridors()`).
- Render corridor arrows (Layer 2) along centerline every 8 tiles.
- Render chokepoint markers at narrowest points.
- Integrate corridor data into `ThreatMap::corridor_pressure`.

**Files:** `src/game/render.rs`, `src/simulation.rs`

### Phase 4: Active Threat Tracking

- Depends on threat_scaling Phase 2 (forest-edge lurking) and Phase 3 (raider entities).
- Detect wolf pack staging (multiple wolves within 10 tiles of a forest cluster edge, not yet advancing).
- Render staging indicators (pulsing forest-edge tiles).
- Render raider march with `B` glyphs and trail.
- Add directional event log messages.

**Files:** `src/game/render.rs`, `src/game/events.rs`

### Phase 5: Landscape Mode Integration

- Add wolf territory color shift to Landscape renderer (desaturated forest tones).
- Add garrison warmth effect to Landscape renderer.
- Add corridor warm-ground tinting.
- Add active threat Landscape effects (red flicker, amber march points).

**Files:** `src/game/render.rs`

### Phase 6: Build Mode Suggested Sites

- When build mode is active and Garrison is selected, render `?` markers at detected chokepoints.
- Filter to chokepoints within 30 tiles of settlement center that lack garrison coverage.
- Markers disappear when a garrison is placed nearby.

**Files:** `src/game/render.rs`, `src/game/build.rs`

## Testing Strategy

**Unit tests:**
- `ThreatMap::update_wolf_territory()` marks forest cluster tiles as territory on a test map with known forest placement.
- `ThreatMap::update_garrison_coverage()` produces coverage values that decay with distance from garrison position.
- Coverage does not bleed through impassable tiles (mountain/water line-of-sight block).
- `exposure` is positive where wolf territory exists without garrison coverage.
- `exposure` is zero or negative where garrison coverage exceeds threat pressure.
- Chokepoint-placed garrison produces higher coverage values along the corridor than open-field garrison.
- Corridor arrow placement produces one arrow every 8 tiles along the centerline.

**Integration tests:**
- On a map with a forest cluster to the north: Threats overlay shows red-brown tint on those forest tiles, not on southern grassland.
- Place a garrison near a northern chokepoint: green coverage tint appears covering the pass. Exposure markers disappear from that direction.
- Spawn a wolf pack at forest edge: active threat indicators appear and move toward settlement over subsequent ticks.
- Deforest northern forest: wolf territory tint retreats to more distant forest clusters after cache recomputation.
- Two garrisons with overlapping coverage: combined coverage value exceeds either alone.

**Visual regression tests (screenshot comparison):**
- Threats overlay on seed 42: screenshot at tick 0 (just territory/corridors) and tick 500 (with garrison coverage) should show clear before/after difference.
- Compare Map mode and Landscape mode threat rendering on same seed to verify both communicate the same spatial information through different visual languages.

## Open Questions

1. **Should wolf territory be visible without the overlay?** A subtle forest darkening in normal play could train the player to notice threatening forests before they ever open the overlay. Risk: visual noise for players who don't care about threat management. Leaning toward overlay-only for territory, normal-play for active threats.

2. **Pulsing/animation in Map mode.** The exposure gap `!` markers pulse every 30 ticks. Is pulsing annoying in a terminal? Should it be a slower cycle (every 60 ticks) or just static bright amber? Need playtesting.

3. **Garrison coverage line-of-sight.** The full line-of-sight check (raycast through terrain) is more realistic but more expensive. Simpler alternative: just use distance, and accept that garrisons "see through" mountains. The visual would be slightly wrong but the computation is cheaper. Phase 1 ships without LOS; add it in Phase 2 if it matters.

4. **ThreatMap resolution.** Should `ThreatMap` be 1:1 with the tile map (256x256 = 65K floats x4 channels = 1MB) or downsampled (128x128, quarter resolution)? Full resolution looks better for tinting but costs more to update. Downsampled is fine for the `!` markers but blocky for tints.

5. **Corridor arrow density.** Every 8 tiles is a guess. On a 60-tile corridor that is 7-8 arrows, which might feel cluttered. Every 12 tiles gives 5 arrows. Need to see it on screen.

## References

- `src/game/render.rs` -- `draw_threat_overlay()` (current implementation, lines 1348-1438)
- `src/game/render.rs` -- Territory overlay tinting (lines 586-615, reference for background blend approach)
- `src/game/mod.rs` -- `OverlayMode` enum (line 129)
- `src/game/build.rs` -- `compute_defense_rating()` (line 211)
- `src/simulation.rs` -- `InfluenceMap` (line 902, structural reference for `ThreatMap`)
- `docs/design/threat_scaling.md` -- Geographic spawn points, chokepoint detection, corridor scanning
- `docs/game_design.md` -- Pillar 4 Rich tier: "wolf territory shown as subtle color shift"
