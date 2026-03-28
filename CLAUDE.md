# terrain-gen-rust

## Build & Test

```bash
cargo test              # run all tests (~199)
cargo run --release     # play the game in terminal
cargo build             # debug build
```

## Module Structure

```
src/
  main.rs              # Terminal input loop, crossterm setup
  lib.rs               # Crate root, re-exports
  renderer.rs          # Renderer trait, Color, Cell types
  crossterm_renderer.rs # Terminal renderer implementation
  headless_renderer.rs # In-memory renderer for testing/AI harness
  tilemap.rs           # TileMap, Camera, Terrain enum
  terrain_gen.rs       # Noise-based terrain generation
  simulation.rs        # DayNight, seasons, water, moisture, vegetation, influence, traffic maps
  ecs/                 # Entity-Component-System (hecs 0.11)
    mod.rs             # Re-exports + unit tests (~60 tests)
    components.rs      # All structs/enums: Position, Creature, BuildingType, etc.
    systems.rs         # All system_* functions (AI, movement, hunger, death, farms, etc.)
    ai.rs              # AI helpers: villager/predator/prey behavior (pub(super))
    spawn.rs           # Entity spawn helpers
    serialize.rs       # World serialization for save/load
  game/                # Game state and orchestration
    mod.rs             # Game struct, new(), step(), tests (~36 tests)
    render.rs          # All draw_* methods (panel, overlays, debug view)
    events.rs          # Random event system (drought, harvest, migration, wolf surge)
    save.rs            # Save/load to JSON
    build.rs           # Building placement, auto-build, influence, traffic, population
```

## Key Conventions

- **hecs ECS**: Queries use `world.query::<&Component>().iter()` (shared) or `world.query_mut::<&mut Component>()` (exclusive)
- **Visibility**: AI helpers use `pub(super)`, build/event/render methods use `pub(super)` where called from mod.rs
- **Re-exports**: Both `ecs/mod.rs` and `game/mod.rs` use `pub use` so external code uses `crate::ecs::Thing` / `crate::game::Thing`
- **Tests**: Unit tests live in each module's `mod.rs`; integration tests in `tests/integration.rs`

## Design Principles

- **Player sets direction, systems execute.** No manual work assignments. Villagers self-organize based on what's built.
- **Placement IS the instruction.** Building a farm tells villagers to farm. Building a garrison defends. No priority sliders.
- **Overlays over UI.** Information conveyed visually (task colors, threat zones, traffic heat) rather than complex menus.
- **Roads auto-build from traffic.** Don't add manual road placement.

## Game Loop

`Game::step()` in `game/mod.rs` runs: input handling -> ECS systems (hunger, AI, movement, breeding, raids, death, farms, processing) -> simulation (water, vegetation, day/night) -> rendering.
