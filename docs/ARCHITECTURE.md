# Architecture

## Overview

43K lines of Rust. Terminal-based settlement simulation with terrain generation, ECS villager AI, and atmosphere/hydrology systems.

## File Map

### Core (src/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `lib.rs` | 12 | Crate root, module declarations | OK |
| `main.rs` | 1040 | CLI, terminal loop, --play/--showcase/--terrain modes | OK |
| `renderer.rs` | ~100 | Renderer trait, Color, Cell types | OK |
| `crossterm_renderer.rs` | ~160 | Terminal renderer implementation | OK |
| `headless_renderer.rs` | ~200 | In-memory renderer for testing | OK |

### Simulation (src/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `simulation.rs` | **4844** | **NEEDS SPLIT.** Contains 11 different systems: WaterMap, MoistureMap, VegetationMap, DayNightCycle, WindField, ScentMap, TrafficMap, SoilFertilityMap, InfluenceMap, ThreatMap, ExplorationMap, Season, SimConfig | **BAD** |
| `pipe_water.rs` | 1115 | New pipe model water (8-directional flux). Standalone, clean. | OK |
| `terrain_gen.rs` | 202 | Perlin noise fBm height generation | OK |
| `terrain_pipeline.rs` | 1702 | 7-stage terrain pipeline: fBm → erosion → hydrology → biomes → soil → resources | OK |
| `tilemap.rs` | 2515 | TileMap, Camera, Terrain enum (26 variants), A* pathfinding, rendering properties | Getting big |

### ECS (src/ecs/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | **7163** | **NEEDS SPLIT.** Re-exports + **230 unit tests** (~5000 lines of tests in one file) | **BAD** |
| `ai.rs` | 2104 | Villager/predator/prey AI behavior functions | Big but coherent |
| `systems.rs` | 2227 | system_ai, system_farms, system_movement, etc. | Big but coherent |
| `components.rs` | 1327 | All ECS components, enums, memory system | Getting big |
| `spatial.rs` | 514 | SpatialHashGrid for O(nearby) queries | OK |
| `groups.rs` | 1030 | GroupManager for agent clustering | OK |
| `ai_arrays.rs` | ~300 | Data-oriented SoA arrays for AI hot path | OK |
| `spawn.rs` | ~250 | Entity spawn helpers | OK |
| `serialize.rs` | 406 | World serialization for save/load | OK |

### Game (src/game/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | **6073** | **NEEDS SPLIT.** Game struct (50+ fields), step(), input handling, 107 tests, milestone system, water cycle, fire system, particle system | **BAD** |
| `render.rs` | **4094** | **NEEDS SPLIT.** 4 render modes (Normal, Map, Landscape, Debug), overlays, query panel, building markers | **BAD** |
| `build.rs` | 2306 | Auto-build, building placement scoring, demolish, outposts, biome reclassification, settlement knowledge | Getting big |
| `events.rs` | 1059 | Event system, threat scaling, milestones | OK |
| `chokepoint.rs` | 689 | Chokepoint detection for defensive positions | OK |
| `dirty.rs` | ~200 | DirtyMap for render optimization | OK |
| `save.rs` | ~200 | Save/load to JSON | OK |

### Pathfinding (src/pathfinding/)

| File | Lines | Purpose | Health |
|------|-------|---------|--------|
| `mod.rs` | ~10 | Module root | OK |
| `graph.rs` | 778 | NavGraph for hierarchical A* | OK |
| `region.rs` | ~400 | NavRegion for zone-based connectivity | OK |
| `flow_field.rs` | 685 | FlowFieldRegistry for shared destinations | OK |

## Refactor Priorities

### Priority 1: Split simulation.rs (4844 lines → ~8 files)

`simulation.rs` has 11 unrelated systems crammed together. They should each be their own module:

```
src/simulation/
  mod.rs           — re-exports, SimConfig
  water_map.rs     — WaterMap (legacy, to be removed when pipe_water takes over)
  moisture.rs      — MoistureMap
  vegetation.rs    — VegetationMap
  day_night.rs     — DayNightCycle, Season, lighting
  wind.rs          — WindField (Stam solver)
  scent.rs         — ScentMap (danger/home scent)
  traffic.rs       — TrafficMap
  soil_fertility.rs — SoilFertilityMap
  maps.rs          — InfluenceMap, ThreatMap, ExplorationMap (small, can share)
```

**Why:** When changing wind, you don't want to scroll past 4000 lines of unrelated code. Also makes parallel agent work safer — different files = no merge conflicts.

### Priority 2: Split game/mod.rs (6073 lines → ~5 files)

The Game struct's step() function is doing too much. Extract:

```
src/game/
  mod.rs           — Game struct, new(), step() (orchestration only)
  input.rs         — GameInput enum, input handling
  water_cycle.rs   — Rain, pipe_water stepping, sediment, wind moisture
  fire.rs          — Fire ignition, spread, burnout
  particles.rs     — Particle spawning for buildings/villagers/wind
  tests.rs         — The 107 tests (or tests/ directory)
```

### Priority 3: Split ecs/mod.rs (7163 lines → tests extracted)

Move the 230 tests (~5000 lines) to `src/ecs/tests.rs` or `src/ecs/tests/` directory. Keep `mod.rs` as just re-exports.

### Priority 4: Split game/render.rs (4094 lines → ~4 files)

Each render mode is independent:

```
src/game/render/
  mod.rs           — shared drawing helpers, draw dispatch
  normal.rs        — draw() Normal mode
  map.rs           — draw_map_mode()
  landscape.rs     — draw_landscape_mode()
  overlays.rs      — all overlay drawing (wind, threats, traffic, etc.)
  query.rs         — query panel
```

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

### Water (confused)
Currently THREE water systems:
- `WaterMap` (old heightfield, in simulation.rs) — still runs for moisture
- `PipeWater` (new pipe model, pipe_water.rs) — used for rendering
- `Terrain::Water` (biome classification) — static from world-gen

**Should be:** ONE water system (PipeWater). Terrain::Water is just a biome hint. WaterMap should be removed. Moisture should read from PipeWater.

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

Currently blended in render.rs with complex logic that differs per render mode.

**Should be:** One `tile_color(x, y, game_state) → (fg, bg)` function that all modes call, with mode-specific overrides for lighting/texture only.
