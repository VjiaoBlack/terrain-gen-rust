# Dirty-Rect Rendering

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 5 (Scale Over Fidelity)*

## Problem

Every frame, `Game::draw()` in `src/game/render.rs` writes every visible screen cell into the `CrosstermRenderer`'s front buffer -- terrain, vegetation, water, overlays, entities, particles, weather, panel, minimap. The renderer's `flush()` then diffs `front` vs `back` and only emits crossterm commands for changed cells. This double-buffer diff (in `src/crossterm_renderer.rs`, lines 115-158) is already a form of dirty-rect rendering at the terminal output layer.

But the work *above* the renderer -- computing what goes into each cell -- is not skipped for unchanged tiles. On a 120x50 terminal with a 20-column panel, the map viewport is ~50x50 world tiles (100x50 screen cells at `CELL_ASPECT=2`). Every frame iterates all 5,000 screen cells through terrain lookup, vegetation overlay, water overlay, lighting, and weather -- plus a full ECS query for entities. At 60fps that is 300,000 terrain lookups/sec and 300,000 lighting computations/sec, even though on a typical frame only 30-100 cells actually change (a few moving entities, a weather particle, a building under construction).

The existing double-buffer catches this at the terminal I/O level, so crossterm only redraws changed cells. But all the CPU work to *decide* what each cell contains still happens. At scale (500+ entities, weather, particles), the draw phase becomes a measurable fraction of the 16ms frame budget.

### What actually changes between frames

| Source | Cells affected | Frequency |
|--------|---------------|-----------|
| Entity movement | 2 per entity (old pos + new pos) | Every tick an entity moves |
| Building construction | 1-9 per building | Only during build timer countdown |
| Weather particles | 1 per particle | Every frame when raining/snowing |
| Smoke/effect particles | 1 per particle | Every frame near active buildings |
| Water shimmer animation | All visible water tiles | Every frame (Landscape Mode only) |
| Vegetation growth | 1 per changed tile | Every ~50 ticks |
| Day/night lighting | All visible tiles | When sun angle changes (Landscape Mode only) |
| Panel text | ~20 cells | When stats change |
| Camera scroll | **All visible tiles** | On arrow key input |

On a quiet frame with no camera movement (the common case when watching the simulation), fewer than 1% of screen cells change. On a busy frame with 50 moving entities and rain, maybe 5%. A camera scroll invalidates everything.

## Design

### Layer 1: World-Space Dirty Tileset

Track which *world tiles* have changed since last frame. The renderer already operates in screen space, but changes originate in world space (an entity moved from tile (40,30) to (41,30)). Tracking in world space means camera scrolling can be handled separately and cleanly.

#### Data structure

```rust
pub struct DirtyMap {
    /// Bitset: one bit per world tile. `true` = needs redraw.
    bits: Vec<u64>,
    width: usize,
    height: usize,
    /// When true, skip per-tile checks -- everything is dirty.
    all_dirty: bool,
}
```

A 256x256 map needs 65,536 bits = 1,024 `u64`s = 8 KB. Trivial memory cost. The bitset allows bulk operations: `mark_all_dirty()` is a single memset, `is_dirty(x, y)` is a single bit test, `mark_dirty(x, y)` is a single bit-or.

```rust
impl DirtyMap {
    pub fn new(width: usize, height: usize) -> Self { ... }

    pub fn mark_dirty(&mut self, x: usize, y: usize) { ... }

    pub fn mark_rect_dirty(&mut self, x: usize, y: usize, w: usize, h: usize) { ... }

    pub fn mark_all_dirty(&mut self) {
        self.all_dirty = true;
    }

    pub fn is_dirty(&self, x: usize, y: usize) -> bool {
        self.all_dirty || /* test bit */
    }

    pub fn clear(&mut self) {
        self.all_dirty = false;
        self.bits.fill(0);
    }
}
```

#### Dirty sources

Each system that modifies visual state marks the affected world tiles dirty:

| System / Event | What to mark |
|---------------|-------------|
| Entity movement (`system_move`) | Old tile + new tile |
| Entity spawn / despawn | Tile at entity position |
| Building placed / demolished | All tiles in building footprint |
| Farm growth tick | Farm tile |
| Terrain modification (mining, deforestation) | Modified tile |
| Vegetation growth (`step_vegetation`) | Changed tile |
| Water level change (`step_water`) | Changed tile |
| Particle spawn / move / expire | Tile at particle position (old + new) |
| Weather toggle | `mark_all_dirty()` |
| Day/night transition (Landscape Mode) | `mark_all_dirty()` |
| Overlay mode change | `mark_all_dirty()` |
| Render mode toggle (Map/Landscape) | `mark_all_dirty()` |
| Window resize | `mark_all_dirty()` (handled by `CrosstermRenderer::resize()` already) |

Most of these are 1-2 tiles. The `mark_all_dirty()` cases are infrequent (mode toggles, day/night transitions happen once every ~200 ticks).

### Layer 2: Camera Scroll Handling

Camera scrolling is the hard case. When the player presses an arrow key, every visible tile changes because the viewport shifted. Naive approach: `mark_all_dirty()` on every scroll. This is correct but defeats the optimization for the common pattern of "scroll a bit, then watch."

#### Approach: Track previous camera position

```rust
pub struct Game {
    // ... existing fields ...
    dirty: DirtyMap,
    prev_camera_x: i32,
    prev_camera_y: i32,
}
```

At the start of `draw()`:

```rust
if self.camera.x != self.prev_camera_x || self.camera.y != self.prev_camera_y {
    self.dirty.mark_all_dirty();
    self.prev_camera_x = self.camera.x;
    self.prev_camera_y = self.camera.y;
}
```

This is the simplest correct approach. Scrolling frames are already expensive (the terminal must redraw everything), so skipping the optimization on scroll frames costs nothing extra. The optimization pays off on the *stationary* frames between scrolls, which is where the player spends most of their time watching the simulation.

**Future refinement (not in initial implementation):** For a 1-tile scroll, we could blit the existing buffer contents with an offset and only redraw the newly exposed row/column. The `CrosstermRenderer` double-buffer already has the previous frame's data in `back`. This would make scrolling nearly free but adds significant complexity. File under "measure first."

### Layer 3: Skip Unchanged Tiles in draw()

The core optimization. In `Game::draw()`, the terrain/vegetation/water/lighting loops gain an early-continue:

```rust
// In the terrain drawing loop:
for sy in 0..h {
    for sx in panel_w..w {
        let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
        let wy = self.camera.y + sy as i32;

        // DIRTY-RECT CHECK: skip clean tiles
        if !self.dirty.is_dirty(wx as usize, wy as usize) {
            continue;
        }

        // ... existing terrain lookup, lighting, draw() call ...
    }
}
```

The same check is added to the vegetation overlay loop, water overlay loop, and territory overlay loop. All of these iterate the same screen-space grid and can share the same dirty check.

Entity rendering is different -- it iterates entities, not tiles. Entities on clean tiles still need to be drawn if they moved *onto* that tile this frame. But entity movement already marks both old and new tiles dirty, so the entity rendering loop doesn't need a dirty check. It draws all visible entities as before, and the dirty marks ensure the terrain underneath gets redrawn too.

### Layer 4: Panel and Minimap

The side panel and minimap are screen-space elements that don't map to world tiles. These are handled separately:

- **Panel**: Already cheap (20 columns of text). No dirty tracking needed. Redraw every frame.
- **Minimap**: Only changes when terrain is modified or camera moves. Redraw every frame (it's small, ~20x10 cells).
- **Status bar**: Redraw every frame (1 row).

These collectively cost <500 cells/frame. Not worth optimizing.

### Lifecycle in Game::step()

```
1. ECS systems run (movement, AI, hunger, etc.)
   -> Each system marks dirty tiles as side effect
2. Simulation steps (water, vegetation, day/night)
   -> Mark dirty tiles for changed cells
3. draw() begins
   -> Check camera moved -> mark_all_dirty() if so
   -> For each screen cell, skip if world tile is clean
   -> Draw panel, minimap, status bar unconditionally
   -> Entity rendering draws all visible entities (their tiles are already dirty)
4. renderer.flush()
   -> CrosstermRenderer diffs front vs back (existing behavior, unchanged)
5. dirty.clear()
   -> Reset all bits for next frame
```

Step 5 happens *after* flush, not after draw. This ensures the dirty map reflects exactly one frame's worth of changes.

## Interaction with Map Mode vs Landscape Mode

**Map Mode** benefits the most from dirty-rect rendering. Map Mode has no lighting, no water shimmer, no weather particles, and no seasonal tinting. The only things that change are entity positions and building construction. A typical Map Mode frame with 30 villagers dirties ~60 tiles out of 5,000 visible. That is a 98% reduction in terrain computation.

**Landscape Mode** benefits less because water shimmer animation and day/night lighting touch many tiles per frame. However:
- Water shimmer only affects water tiles. On most maps, water is <15% of visible area. Only water tiles need the shimmer dirty mark.
- Day/night transitions happen infrequently (every ~200 ticks). Between transitions, Landscape Mode is as static as Map Mode except for entities and weather.
- Weather particles dirty individual tiles, not the whole screen. Even heavy rain only affects ~100-200 random tiles per frame.

Net: Map Mode sees ~98% skip rate on static frames. Landscape Mode sees ~70-85% skip rate between lighting transitions.

## Expected Performance Improvement

### Current cost model (no dirty-rect)

Per frame, assuming 100x50 screen cell viewport (50x50 world tiles at aspect 2):

| Operation | Cells | Cost per cell | Total |
|-----------|-------|--------------|-------|
| Terrain lookup + glyph | 5,000 | ~20ns (array index + match) | 100us |
| Vegetation overlay | 5,000 | ~15ns (bounds check + density lookup) | 75us |
| Water overlay | 5,000 | ~15ns (bounds check + depth lookup) | 75us |
| Lighting (Landscape) | 5,000 | ~30ns (sun angle + color multiply) | 150us |
| Weather particles | ~200 | ~10ns | 2us |
| Entity query + draw | ~50 entities | ~200ns (ECS query + screen transform) | 10us |
| **Total draw phase** | | | **~410us** |

At 60fps this is 2.5% of the 16ms budget. Comfortable today with 30 villagers. But draw cost scales with viewport size (larger terminals), entity count (500+ entities means bigger ECS queries), and weather density. On a 200x60 terminal with 500 entities and rain, draw could reach 1-2ms.

### With dirty-rect (stationary camera, Map Mode)

| Operation | Cells | Cost per cell | Total |
|-----------|-------|--------------|-------|
| Dirty check (all tiles) | 5,000 | ~2ns (bit test) | 10us |
| Terrain lookup (dirty only) | ~60 | ~20ns | 1.2us |
| Vegetation overlay (dirty only) | ~60 | ~15ns | 0.9us |
| Entity draw (unchanged) | ~50 | ~200ns | 10us |
| Panel + minimap (unchanged) | ~500 | ~10ns | 5us |
| **Total draw phase** | | | **~27us** |

That is a ~15x reduction in draw cost. The savings compound at scale: with 500 entities the dirty tile count rises to ~1,000 (still 80% skip rate), and the entity query itself becomes the dominant cost -- which is the right bottleneck to have, because entity rendering scales with entity count, not viewport size.

### Scroll frames

Scroll frames see zero improvement (everything is dirty). This is fine. Scroll frames are already handled efficiently by the existing double-buffer diff in `CrosstermRenderer::flush()`, which only emits terminal commands for cells that actually changed after the scroll. The user typically scrolls for 0.5-2 seconds then watches for 30+ seconds. The optimization pays for the watching time.

## Implementation Plan

### Phase 1: DirtyMap struct + camera-scroll invalidation

1. Add `DirtyMap` to `src/game/mod.rs` (or a new `src/game/dirty.rs` if it exceeds ~80 lines).
2. Initialize in `Game::new()` with map dimensions.
3. Track `prev_camera_x`, `prev_camera_y`. On camera change, `mark_all_dirty()`.
4. Call `dirty.clear()` at end of `Game::step()` after flush.
5. No rendering changes yet -- just the infrastructure. All frames start as `all_dirty = true` from `mark_all_dirty()` in `new()`.

### Phase 2: Mark dirty from ECS systems

Add `dirty.mark_dirty(x, y)` calls to:
- `system_move` in `src/ecs/systems.rs` -- mark old position before move, new position after.
- Entity spawn functions in `src/ecs/spawn.rs` -- mark spawn tile.
- Building placement in `src/game/build.rs` -- mark building footprint.
- `step_vegetation` in `src/simulation.rs` -- mark tiles where density changed.
- `step_water` in `src/simulation.rs` -- mark tiles where depth changed.
- Particle spawn/move/expire in `src/game/render.rs` -- mark particle positions.

The `DirtyMap` needs to be passed through (or accessed via `&mut Game`) at each call site. Since `Game` already owns the ECS world and simulation state, the simplest approach is to have systems return a `Vec<(usize, usize)>` of dirtied positions, or pass `&mut DirtyMap` alongside the world.

### Phase 3: Skip clean tiles in draw()

Add the `if !self.dirty.is_dirty(wx, wy) { continue; }` check to:
- Terrain rendering loop
- Vegetation overlay loop
- Water overlay loop
- Territory overlay loop
- Weather rendering loop (check world tile under particle)

Do NOT add dirty checks to:
- Entity rendering (entities on dirty tiles are already guaranteed dirty)
- Panel rendering (always cheap)
- Minimap rendering (always cheap)
- Build cursor / query cursor (always cheap)

### Phase 4: Remove initial mark_all_dirty per frame

Currently the system works correctly because `mark_all_dirty()` is called in `new()`. In Phase 4, remove that crutch. The first frame after `new()` is all-dirty (correct). Subsequent frames are only dirty where marked. This is where the optimization actually activates.

Test by running `--play --ticks 500` and verifying identical output with and without dirty-rect enabled. The headless renderer captures the final frame; it should be bit-identical.

## Testing

- **Correctness**: Run `cargo run --release -- --play --ticks 500` with dirty-rect enabled and disabled. Capture final frame from headless renderer. Assert pixel-identical output. This is the critical test -- any rendering glitch means a dirty mark is missing.
- **Mark coverage**: Unit test that every `BehaviorState` transition that changes visual appearance also marks the entity's tile dirty. Enumerate state transitions, check dirty bit.
- **Camera scroll**: Unit test that scrolling marks all dirty, and the frame after scroll with no movement marks only entity tiles dirty.
- **Performance**: Benchmark `Game::draw()` with 500 stationary entities and no camera movement. Compare time with and without dirty-rect. Target: >10x reduction in draw call count to renderer.
- **Edge cases**:
  - Entity moves off-screen: old tile is dirty but not in viewport (skip is correct).
  - Entity moves on-screen from off-screen: new tile is dirty and in viewport (drawn correctly).
  - Two entities swap tiles in one tick: both old and new positions are dirty (drawn correctly).
  - Window resize during dirty frame: `resize()` already clears both buffers and forces full redraw.

## Open Questions

1. **DirtyMap ownership.** Systems in `src/ecs/systems.rs` currently take `&mut World` but not `&mut Game`. Passing `&mut DirtyMap` to every system that moves entities is invasive. Alternative: systems return a list of moved positions, and `Game::step()` marks them dirty in bulk. Less invasive but means an extra allocation per tick. Worth benchmarking both approaches.

2. **Water shimmer in Landscape Mode.** The shimmer animation cycles every N frames, touching all water tiles. Options: (a) mark all water tiles dirty every frame (simple, loses some benefit on water-heavy maps), (b) track shimmer phase and only mark dirty on phase change, (c) skip shimmer unless water is already dirty for another reason. Option (b) is probably the right balance.

3. **Day/night granularity.** Lighting changes every tick as the sun moves. In Landscape Mode, should we mark all tiles dirty every tick (defeating the optimization) or quantize lighting into steps and only mark dirty on step transitions? The current lighting already uses stepped values (8 or 16 levels), so step transitions are natural dirty triggers.

4. **Overlay mode interaction.** Some overlays (task overlay, traffic overlay) tint every visible tile. Switching overlays should `mark_all_dirty()`. But while an overlay is active, do overlay values change often enough to matter? Traffic overlay changes slowly (every ~10 ticks). Task overlay changes with entity state. Needs profiling.

5. **Should DirtyMap be screen-space or world-space?** This doc proposes world-space because changes originate in world coordinates. But screen-space would avoid the coordinate transform in the dirty check. With `CELL_ASPECT=2`, one world tile maps to 2 screen columns, so a world-space dirty bit covers 2 screen cells automatically. World-space is simpler and correct. Screen-space would only matter if we had screen-space-only effects (e.g., panel animations), which we don't.
