# terrain-gen-rust

Terminal-based terrain simulation and ecosystem sandbox, built in Rust. Features procedural terrain generation, real-time water/erosion simulation, day/night lighting with shadow raytracing, and an AI-driven prey/predator ecosystem.

Designed as an AI development harness: headless renderer, frame serialization, programmatic input injection, and 85+ automated tests enable rapid iteration with AI assistance.

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange)

## Features

**Terrain & Simulation**
- Procedural terrain via Perlin fBm — 6 terrain types (water, sand, grass, forest, mountain, snow)
- Real-time water flow with gradient descent, evaporation, and pooling
- Hydraulic erosion simulation
- Moisture propagation and vegetation growth/decay
- Day/night cycle with Blinn-Phong lighting and shadow raytracing

**Ecosystem AI**
- Prey (rabbits) seek berry bushes when hungry, eat, then flee home to dens
- Predators (wolves) hunt visible prey that aren't safe at home
- Hunger cycle drives behavior — creatures eat roughly once per in-game day
- State machine AI: Wander / Seek / Eat / FleeHome / AtHome / Hunt / Idle

**Engine**
- ECS architecture (hecs) with Position, Velocity, Sprite, Behavior, Creature components
- Grid-based collision with axis-separated wall sliding and bounce
- 2:1 aspect ratio correction for square-looking terminal tiles
- Double-buffered crossterm rendering at 60fps (hybrid sleep + spin-wait)
- Terminal resize handling

**AI Harness**
- Headless renderer for max-speed simulation and testing
- `Game::step()` / `Game::step_headless()` for single-tick advance
- `GameInput` enum decoupled from terminal input
- `run_script()` feeds action sequences, returns `FrameSnapshot` series
- `FrameSnapshot::diff()` for cell-level change detection
- JSON serialization of frames via serde

## Controls

| Key | Action |
|-----|--------|
| Arrow keys | Scroll camera |
| `r` | Toggle rain |
| `e` | Toggle erosion |
| `t` | Toggle day/night cycle |
| `v` | Toggle debug view |
| `k` | Toggle query/inspect mode |
| `d` | Drain all water |
| `q` | Quit (or exit query mode) |

**Query mode** (`k`): Move cursor with `WASD`. Shows tile info (terrain, height, water, moisture, vegetation) and entity details (species, hunger, AI state, speed, home location).

## Building & Running

```bash
cargo run --release
```

## Testing

```bash
cargo test
```

85 tests cover terrain generation, water simulation, day/night lighting, ECS systems, collision, AI behavior, ecosystem interactions, and the headless harness.

## Architecture

```
src/
  main.rs              — Entry point, input mapping, game loop timing
  game.rs              — Game state, rendering, query panel
  ecs.rs               — Components, systems, AI (prey/predator/wander)
  simulation.rs        — Water, erosion, moisture, vegetation, day/night
  tilemap.rs           — Tile map, terrain types, camera
  terrain_gen.rs       — Procedural generation (Perlin fBm)
  renderer.rs          — Renderer trait
  crossterm_renderer.rs — Terminal backend with double buffering
  headless_renderer.rs  — No-op renderer for testing/AI
```

## Dependencies

- **hecs** — ECS
- **crossterm** — Terminal rendering
- **noise** — Perlin noise generation
- **rand** — RNG for AI behavior
- **serde / serde_json** — Frame serialization
- **anyhow** — Error handling
