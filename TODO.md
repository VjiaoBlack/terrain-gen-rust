# terrain-gen-rust TODO

## Foundation [done]
- [x] Renderer trait + crossterm backend with double buffering
- [x] Game loop with configurable FPS
- [x] Input handling
- [x] Optional background color (None = terminal default)

## AI Harness [done]
- [x] Headless renderer — no-op `Renderer` impl for max-speed sim and testing
- [x] Frame serialization — serde FrameSnapshot (tick, size, text, cells) → JSON
- [x] Step mode — `Game::step()` and `Game::step_headless()` for single-tick advance
- [x] Input abstraction — `GameInput` enum decoupled from crossterm
- [x] Test harness — `cargo test` runs headless game scenarios (19 tests)
- [x] Programmatic input injection — `run_script()` feeds action sequences, returns snapshots
- [x] Frame diffing — `FrameSnapshot::diff()` returns cell-level `FrameDiff`

## Engine Core
- [x] ECS — hecs with Position, Velocity, Sprite components + movement/render systems
- [x] Terminal resize handling

## Game Systems
- [x] Tile map — 2D grid world, camera/viewport, scrolling (256x256, Camera with clamp)
- [x] Terrain generation — Perlin fBm with configurable octaves, 6 terrain types
- [x] Entity rendering — entities drawn on map with camera offset
- [x] Physics/collision — grid walkability, axis-separated slide+bounce
- [x] Planning/AI — Wander/Seek/Idle state machine, terrain-aware direction picking

## Terrain Simulation
- [x] Water flow — gradient descent, 8 neighbors, evaporation, pooling
- [x] Rain — toggleable random water drops
- [x] Erosion — toggleable, distributed kernel modifies terrain heights
- [x] Water rendering — depth-colored ~≈ characters over terrain
- [x] Interactive controls — [r] rain, [e] erosion, [d] drain
- [x] Moisture propagation — spreads from water, drives vegetation
- [x] Vegetation growth/decay — responds to moisture bands
- [x] Day/night cycle — sun position, color tinting, shadow raytracing, [t] toggle

## Ecosystem AI [done]
- [x] Berry bushes — static food source entities on grass tiles
- [x] Dens — prey home base, safe from predators
- [x] Prey AI — hungry→seek food→eat→go home, flee if predator nearby
- [x] Predator AI — hungry→hunt visible prey (not at den)→eat
- [x] Hunger cycle — increases over time, eating resets it

## Settlement System [done]
- [x] Villager AI — utility-based (flee > eat > build > gather > wander)
- [x] Resource system — Food/Wood/Stone with HUD display
- [x] Building system — build mode [b], Hut/Wall/Farm types, ghost preview
- [x] Farm system — seasonal growth, 4 visual stages, auto-harvest
- [x] Influence maps — organic territory from villagers/buildings
- [x] Population growth — reproduction with food/housing requirements
- [x] Seasonal calendar — Spring/Summer/Autumn/Winter with gameplay effects
- [x] Ecosystem breeding — prey at dens, wolves in open, Spring/Summer only
- [x] Wolf-villager combat — wolves attack villagers when desperate
- [x] Stone deposits — mineable entities near settlement
- [x] Event notifications — on-screen display for deaths, births, harvests
- [x] Game over — detection + overlay with survival stats
- [x] 117 tests

## Later
- [ ] Save/load game state (serde in place, hecs World needs manual serialization)
- [ ] Water cycle rework — slower accumulation, evaporation balance, seasonal floods
- [ ] notcurses backend swap
- [ ] rayon parallelism where needed
- [ ] ratatui UI panels (inventory, stats, etc.)
- [ ] Villager pathfinding improvements (currently re-targets every tick in Seek)
- [ ] Difficulty scaling / game balance tuning
