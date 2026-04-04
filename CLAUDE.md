# terrain-gen-rust

## Development Rules (READ FIRST)

**These are hard requirements, not suggestions.**

### Before ANY code change:
1. **Understand the full system.** Read the relevant code paths end-to-end. Trace the data flow. Understand WHY the current code is the way it is before modifying it.
2. **Write a failing test FIRST.** Define what "working" looks like as a test. Watch it fail. Then fix the root cause. If you can't write a test, write a diagnostic that prints actual vs expected values.
3. **One change at a time.** Make one focused change, verify it works (test + diagnostic), commit. Don't bundle 5 fixes in one commit.

### After EVERY code change:
4. **Verify with data, not assumptions.** Run `cargo test --lib`. Run `--diagnostics` mode. Check actual numbers. Don't assume a change works because it compiles.
5. **Check for regressions.** Does the change break something else? Run a quick playtest across 2-3 seeds.

### Code Review:
6. **Every significant diff gets reviewed before merge.** A review agent (or the orchestrator) checks: logic errors, design violations, magic fixes, untested paths, data flow correctness. See `docs/workflow.md` for the full review protocol.

### Never do this:
- Don't make a change, then immediately make another change to fix the first change, then another to fix that. **Stop. Diagnose. Understand. Then fix once.**
- Don't add "infinite sources" or "magic spawning" to paper over a bug. Find the root cause.
- Don't change constants (thresholds, rates, levels) without checking what values the system actually produces. Add a diagnostic test first.
- Don't propose a fix that contradicts the design docs. Check `docs/game_design.md` and `docs/design/` first.

## Key Docs
- `docs/ARCHITECTURE.md` — file map, system health, data flow issues, refactor plan
- `docs/game_design.md` — design pillars (ranked), success criteria, anti-goals
- `docs/workflow.md` — full development pipeline including code review protocol
- `INDEX.md` — complete doc inventory with links

## Build & Test

```bash
cargo test --lib        # fast: lib tests only (~7s, ~190 tests) — use during development
cargo test              # full: all tests including integration (~60s, ~225 tests)
cargo test --features lua  # with Lua scripting tests
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

## Playing & Testing Non-Interactively

```bash
cargo run --release -- --play --ticks 500                    # work mode: plain text frame
cargo run --release -- --play --inputs "tick:500,ansi"       # fun mode: ANSI color frame
cargo run --release -- --play --inputs "tick:100,frame,input:ScrollDown,tick:100,frame"  # scripted
cargo run --release -- --screenshot --width 80 --height 30   # single ANSI screenshot
```

## Buildings & Production Chains

| Building   | Cost        | Recipe              | Notes                        |
|------------|-------------|---------------------|------------------------------|
| Farm       | 5w 1s       | (growth→food)       | Villagers tend it, harvests auto-collect |
| Hut        | 10w 4s      | —                   | Housing, villagers sleep here at night |
| Wall       | 2w 2s       | —                   | Defensive, blocks movement   |
| Workshop   | 8w 3s       | 2 wood → 1 plank    | Needs worker + wood          |
| Smithy     | 5w 8s       | 2 stone → 1 masonry | Needs worker + stone         |
| Granary    | 6w 4s       | 3 food → 2 grain    | Preserves food for winter    |
| Bakery     | 8w 4s 2p    | 2 grain + 1 wood → 3 bread | Prevents plague events |
| Garrison   | 4w 6s 2m    | —                   | Defends against wolf raids   |
| Bridge     | 8w 4s       | —                   | River crossing, player-placed on Water tiles |
| Stockpile  | —           | —                   | Auto-placed at start, resource depot |

**Key**: w=wood, s=stone, p=planks, m=masonry

## Controls

| Key     | Action              | Key     | Action              |
|---------|---------------------|---------|---------------------|
| arrows  | scroll camera       | `b`     | toggle build mode   |
| `k`     | query/inspect       | `o`     | cycle overlay       |
| `f`     | cycle speed (1/2/5x)| `g`     | goto settlement     |
| `a`     | toggle auto-build   | `space` | pause               |
| `q` (x2)| quit               | `s`/`l` | save/load           |

**Build mode**: `wasd` move cursor, `tab` cycle type, `enter` place, `x` demolish

## Terrain & Movement

| Terrain       | Speed | Walkable | A* Cost |
|---------------|-------|----------|---------|
| Road          | 1.5x  | yes      | 0.7     |
| Grass/Floor   | 1.0x  | yes      | 1.0     |
| Bare          | 0.9x  | yes      | 1.0     |
| Sand/Stump    | 0.8x  | yes      | 1.2-1.3 |
| Sapling       | 0.7x  | yes      | 1.4     |
| Forest        | 0.6x  | yes      | 1.7     |
| Snow          | 0.4x  | yes      | 2.5     |
| Mountain      | 0.25x | yes      | 4.0     |
| Water         | —     | **no**   | ∞       |
| BuildingWall  | —     | **no**   | ∞       |

Villagers use A* pathfinding; prey/predators use direct movement.
