# Architecture

## Overview

~48K lines of Rust. Terminal-based settlement simulation with terrain generation, ECS villager AI, and atmosphere/hydrology systems.

## File Map

### Core (src/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `lib.rs` | 18 | Crate root, module declarations | OK |
| `main.rs` | 1225 | CLI, terminal loop, --play/--showcase/--terrain modes | OK |
| `renderer.rs` | 30 | Renderer trait, Color, Cell types | OK |
| `crossterm_renderer.rs` | ~160 | Terminal renderer implementation | OK |
| `headless_renderer.rs` | ~205 | In-memory renderer for testing | OK |
| `world_state.rs` | 74 | WorldState struct -- single source of truth for 0D architecture | OK |

### Simulation (src/simulation/)

Split from the former monolithic `simulation.rs` (4844 lines) into separate modules.

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | 88 | Re-exports, SimConfig | OK |
| `day_night.rs` | 927 | DayNightCycle, Season, lighting | OK |
| `maps.rs` | 807 | InfluenceMap, ThreatMap, ExplorationMap | OK |
| `moisture.rs` | 1867 | MoistureMap | Getting big |
| `scent.rs` | 295 | ScentMap (danger/home scent) | OK |
| `soil_fertility.rs` | 153 | SoilFertilityMap | OK |
| `traffic.rs` | 440 | TrafficMap | OK |
| `vegetation.rs` | 219 | VegetationMap | OK |
| `water_map.rs` | 460 | WaterMap (legacy, to be removed when pipe_water takes over) | OK |
| `wind.rs` | 1540 | WindField (Stam solver) | Getting big |

### Simulation (src/ top-level)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `hydrology.rs` | 504 | Particle-based hydraulic erosion (Nick McDonald's SimpleHydrology port) | OK |
| `pipe_water.rs` | 1157 | Pipe model water (8-directional flux). Standalone, clean. | OK |
| `terrain_gen.rs` | 202 | Perlin noise fBm height generation | OK |
| `terrain_pipeline.rs` | 2232 | 7-stage terrain pipeline: fBm -> erosion -> hydrology -> biomes -> soil -> resources | OK |
| `tilemap.rs` | 2515 | TileMap, Camera, Terrain enum (26 variants), A* pathfinding, rendering properties | Getting big |

### ECS (src/ecs/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | 17 | Re-exports only | OK |
| `tests.rs` | 7164 | ~230 unit tests (extracted from former mod.rs) | OK |
| `ai.rs` | 2104 | Villager/predator/prey AI behavior functions | Big but coherent |
| `systems.rs` | 2227 | system_ai, system_farms, system_movement, etc. | Big but coherent |
| `components.rs` | 1327 | All ECS components, enums, memory system | Getting big |
| `spatial.rs` | 514 | SpatialHashGrid for O(nearby) queries | OK |
| `groups.rs` | 1030 | GroupManager for agent clustering | OK |
| `ai_arrays.rs` | 291 | Data-oriented SoA arrays for AI hot path | OK |
| `spawn.rs` | 290 | Entity spawn helpers | OK |
| `serialize.rs` | 406 | World serialization for save/load | OK |

### Game (src/game/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | 2592 | Game struct, new(), step() orchestration | OK |
| `input.rs` | 281 | GameInput enum, input handling | OK |
| `water_cycle.rs` | 310 | Rain, pipe_water stepping, sediment, wind moisture | OK |
| `fire.rs` | 231 | Fire ignition, spread, burnout | OK |
| `particles.rs` | 227 | Particle spawning for buildings/villagers/wind | OK |
| `tests.rs` | 2927 | Game tests (extracted from former mod.rs) | OK |
| `build.rs` | 2306 | Auto-build, building placement scoring, demolish, outposts, biome reclassification, settlement knowledge | Getting big |
| `events.rs` | 1059 | Event system, threat scaling, milestones | OK |
| `chokepoint.rs` | 689 | Chokepoint detection for defensive positions | OK |
| `dirty.rs` | 165 | DirtyMap for render optimization | OK |
| `save.rs` | 179 | Save/load to JSON | OK |

### Game Rendering (src/game/render/)

Split from the former monolithic `render.rs` (4094 lines) into separate modules.

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | 1300 | Shared drawing helpers, draw dispatch | OK |
| `shared.rs` | 914 | Common rendering utilities | OK |
| `overlays.rs` | 555 | All overlay drawing (wind, threats, traffic, etc.) | OK |
| `normal.rs` | 415 | draw() Normal mode | OK |
| `landscape.rs` | 398 | draw_landscape_mode() | OK |
| `query.rs` | 327 | Query panel | OK |
| `map.rs` | 294 | draw_map_mode() | OK |
| `debug.rs` | 115 | Debug render mode | OK |

### Pathfinding (src/pathfinding/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | 7 | Module root | OK |
| `graph.rs` | 778 | NavGraph for hierarchical A* | OK |
| `region.rs` | 396 | NavRegion for zone-based connectivity | OK |
| `flow_field.rs` | 685 | FlowFieldRegistry for shared destinations | OK |

## Refactor Priorities

### Completed

- **Priority 1 (DONE):** Split `simulation.rs` (4844 lines) into `src/simulation/` with 10 files (~6800 lines total).
- **Priority 2 (DONE):** Split `game/mod.rs` (6073 lines) into `mod.rs` (2592) + `input.rs`, `water_cycle.rs`, `fire.rs`, `particles.rs`, `tests.rs`.
- **Priority 3 (DONE):** Extracted ECS tests from `ecs/mod.rs` (7163 lines) into `ecs/tests.rs` (7164 lines). `mod.rs` is now 17 lines of re-exports.
- **Priority 4 (DONE):** Split `game/render.rs` (4094 lines) into `src/game/render/` with 8 files (~4300 lines total).
- **0D Architecture (DONE):** Added `world_state.rs` (WorldState struct) as canonical simulation state. Game migrated to use WorldState. Terrain::Water now derived from water depth.

### Open

### Priority 5: Clean up tilemap.rs (2515 lines)

Terrain enum has 26 variants with 10+ match blocks each for properties (ch, fg, bg, soil_fg, landscape_fg, etc.). Consider a data-driven approach:

```rust
struct TerrainDef {
    ch: char, fg: Color, bg: Color,
    soil_fg: Color, veg_color: Color,
    speed: f64, cost: f64, walkable: bool,
    // ...
}
static TERRAIN_DEFS: &[TerrainDef] = &[...];
```

## Data Flow Issues

### Water (partially resolved)
Three water systems remain, but the 0D WorldState refactor clarified ownership:
- `WaterMap` (legacy, in simulation/water_map.rs) -- still runs for moisture
- `PipeWater` (pipe model, pipe_water.rs) -- primary water simulation
- `Terrain::Water` -- now **derived** from water depth via WorldState, no longer static from world-gen

**Remaining:** WaterMap should eventually be removed. Moisture should read from PipeWater directly.

### Moisture (broken chain)
- Pipeline computes moisture at world-gen → seeds MoistureMap
- MoistureMap::update() reads from PipeWater + wind
- But moisture decays/persists based on unclear rules
- avg_moisture tracks long-term average but update_average blend rate unclear

**Should be:** Clear flow: water_surface → evaporation (wind picks up) → transport (wind carries) → precipitation (orographic + saturation) → surface water (pipe_water) → moisture for vegetation. Each link verified with tests.

### Rendering (three truths)
- `Terrain::fg()/bg()` — biome-based colors
- `SoilType::ground_fg()` — soil-based colors
- `vegetation_color_from_conditions()` — climate-based vegetation color

Currently blended in game/render/ with complex logic that differs per render mode.

**Should be:** One `tile_color(x, y, game_state) → (fg, bg)` function that all modes call, with mode-specific overrides for lighting/texture only.
