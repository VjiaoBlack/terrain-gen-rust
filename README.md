# terrain-gen-rust

A terminal-based settlement simulation game written in Rust. Watch an autonomous village grow, survive, and interact with a living ecosystem -- all rendered in your terminal.

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange)

![screenshot](screenshot.png)

## Playing

```bash
cargo run --release
```

## Design Philosophy

### The Ant Colony

This game draws inspiration from the space between Civilization and Banished, with Songs of Syx as a key reference. The core idea: **the player sets direction, and the systems execute.**

Your settlement is an ant colony. Villagers self-organize based on what's built and what's needed. The player's only verb is "place building/zone." There are no manual work assignments, no priority sliders, no micromanagement. Fun comes from watching emergent behavior and making strategic decisions about *what* to build and *where*.

### Placement IS the Instruction

Every building is a signal to the AI:
- Place a **Farm** and villagers will tend it, plant crops, harvest food
- Place a **Workshop** and a villager will process Wood into Planks
- Place a **Garrison** and the settlement's defense rating rises, repelling wolf raids
- Place a **Hut** and the population can grow (births require housing surplus)

There are no "assign worker" buttons. Villagers read the settlement state -- what's scarce, what's nearby, what's been built -- and pick the most useful task.

### Observability Over Control

Instead of complex UI panels, the game uses **overlay modes** (`o` key) to visualize systems:
- **Tasks**: color-codes villagers by activity (farming=green, building=yellow, fleeing=red, sleeping=blue)
- **Resources**: highlights food sources, stone deposits, stockpiles
- **Threats**: shows wolf positions, den danger zones, garrison defense coverage
- **Traffic**: heat map of villager foot traffic (high traffic auto-builds roads!)

Debug in-game, not in source code.

## How the Systems Work

### Entity-Component-System (ECS)

Built on [hecs](https://crates.io/crates/hecs), a minimal archetypal ECS. Every creature, building, resource node, and build site is an entity with components:

- **Creature** + **Behavior** + **Position** + **Velocity** + **Sprite**: all living things
- **BuildSite**: construction in progress (tracks progress, assigned workers)
- **FarmPlot**: farm growth state with 4 visual stages
- **ProcessingBuilding**: Workshop/Smithy/Granary/Bakery with recipe and progress
- **FoodSource**, **StoneDeposit**, **Den**: resource nodes and wolf dens

### Villager AI

Each tick, `system_ai` runs for every creature. Villagers use a priority-based task selection:

1. **Flee** if wolves are nearby and settlement is undefended
2. **Sleep** at night if a hut is nearby
3. **Eat** from stockpile if very hungry (prefers grain over raw food)
4. **Work** at assigned processing building
5. **Farm** at assigned farm plot
6. **Build** nearest construction site
7. **Haul** carried resources to stockpile
8. **Gather** nearest resource (prioritized by scarcity)
9. **Wander** if nothing else to do

This creates emergent behavior: when food is low, more villagers naturally shift to gathering food. When a building is placed, nearby idle villagers move to construct it.

### Economy & Production Chains

Raw resources (Food, Wood, Stone) are gathered from the world. Advanced buildings require refined resources:

```
Wood  -> Workshop -> Planks
Stone -> Smithy   -> Masonry
Food  -> Granary  -> Grain (preserved, doesn't spoil in winter)
Grain + Wood -> Bakery -> Bread (highest food value)
```

Garrison requires Planks + Masonry, creating a natural tech tree: you need Workshop + Smithy before you can build defenses. No tech tree UI needed -- the player discovers this through resource requirements.

### Ecosystem

The world has a living ecosystem:
- **Rabbits** breed near dens, flee from wolves, eat at berry bushes
- **Wolves** hunt rabbits (and villagers if undefended), breed in packs
- **Berry bushes** regrow over time, faster near water/moisture
- **Stone deposits** are finite, driving settlement expansion

Wolf packs of 5+ launch coordinated **raids** toward the settlement center. Defense rating (from garrisons, walls, and military skill) determines if the raid is repelled.

### Seasons & Events

A full day/night and seasonal cycle (10-day seasons, 40-day years, 1 day = 1200 ticks):

| Season | Effects |
|--------|---------|
| **Spring** | High vegetation growth, rain, animal breeding, migration events (new villagers arrive if food + housing available) |
| **Summer** | Peak food, low rain, fast farm growth, drought risk (farm yields halved) |
| **Autumn** | Vegetation decays, shorter days, warm orange-brown tint, bountiful harvest chance (farm yields doubled) |
| **Winter** | No growth, 1.8x hunger, food spoilage (grain preserved), wolves target villagers, wolf surge risk |

Events are self-resolving -- the player doesn't "handle" them, they prepare (or don't) by what they built.

### Traffic & Roads

Villager foot traffic is tracked on every tile. When a tile accumulates enough traffic (150+ steps), it automatically converts to a **Road**. Roads give a 1.5x movement speed bonus. This creates organic road networks along the most-used paths -- no manual road placement.

### Influence & Territory

An influence map radiates from buildings and villagers. Buildings can only be placed within your territory (where influence > 0.1). Garrisons project stronger influence, enabling frontier expansion. The subtle blue territory tint is always visible on the map.

### CivSkills

Settlement-wide skills grow from activity and decay from inactivity:
- **Farming**: increases farm growth rate
- **Mining/Woodcutting**: increases gathering speed
- **Building**: bonus construction progress per tick
- **Military**: contributes to defense rating

## Controls

| Key | Action |
|-----|--------|
| Arrow keys | Scroll camera |
| `b` | Toggle build mode (Tab to cycle, Enter to place, wasd to move cursor) |
| `k` | Toggle query/inspect mode |
| `o` | Cycle overlay modes (Tasks, Resources, Threats, Traffic) |
| `a` | Toggle auto-build mode |
| `r` | Toggle rain |
| `e` | Toggle erosion |
| `t` | Toggle day/night cycle |
| `v` | Toggle debug view |
| `Space` | Pause/unpause |
| `d` | Drain all water |
| Mouse click | Place building (build mode) or query tile (normal mode) |
| `q` | Quit |

**Left panel**: Always-visible sidebar showing date/season, resources (Food/Wood/Stone + refined when present), population, skills, clickable building buttons with costs, auto-build toggle, active events, and control hints.

**Query mode** (`k`): Shows tile info (terrain, height, water, moisture, vegetation, influence) and entity details (species, hunger, AI state, speed, home, farm growth, recipe progress).

**Build mode** (`b`): Ghost preview shows green=valid, red=invalid. Must be within settlement influence. Costs displayed in panel.

**Auto-build** (`a`): AI automatically queues farms when food is low, huts when population grows, and walls when wolves approach.

## Architecture

```
src/
  main.rs                 -- Entry point, input mapping, game loop timing
  lib.rs                  -- Crate root, re-exports
  renderer.rs             -- Renderer trait, Color, Cell types
  crossterm_renderer.rs   -- Terminal backend with double buffering at 60fps
  headless_renderer.rs    -- In-memory renderer for testing/AI harness
  tilemap.rs              -- Tile map, terrain types, camera
  terrain_gen.rs          -- Procedural generation (Perlin fBm)
  simulation.rs           -- Water, erosion, moisture, vegetation, day/night, seasons, influence, traffic
  ecs/                    -- Entity-Component-System (hecs)
    components.rs         -- All types: Position, Creature, BuildingType, Resources, etc.
    systems.rs            -- system_* functions (AI, movement, death, farms, processing, raids)
    ai.rs                 -- AI behavior helpers (villager/predator/prey logic)
    spawn.rs              -- Entity spawn helpers
    serialize.rs          -- World serialization for save/load
  game/                   -- Game state and orchestration
    mod.rs                -- Game struct, new(), step() main loop
    render.rs             -- All draw_* methods (panel, overlays, debug view)
    events.rs             -- Random event system
    save.rs               -- Save/load to JSON
    build.rs              -- Building placement, auto-build, influence, traffic, population
```

## Testing

```bash
cargo test    # ~199 tests
```

Tests cover terrain generation, water simulation, day/night lighting, all ECS systems, AI behavior, ecosystem interactions, building system, seasons, farming, breeding, influence maps, production chains, events, overlays, save/load, and full game lifecycle (1000+ tick survival runs).

The headless renderer enables full game simulation in tests without a terminal.

## AI Harness

The game includes a headless mode that captures full frame snapshots as structured data:

- `Game::step()` / `Game::step_headless()` for single-tick advance
- `run_script()` feeds action sequences, returns `FrameSnapshot` series
- `FrameSnapshot::diff()` for cell-level change detection
- JSON serialization of frames via serde
- Decoupled `GameInput` enum for programmatic control

## Dependencies

- **hecs** -- ECS
- **crossterm** -- Terminal rendering
- **noise** -- Perlin noise generation
- **rand** -- RNG for AI behavior
- **serde / serde_json** -- Serialization
- **anyhow** -- Error handling
