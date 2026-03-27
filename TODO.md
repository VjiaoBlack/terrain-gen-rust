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
- [ ] Tile map — 2D grid world, camera/viewport, scrolling
- [ ] Terrain generation — port Perlin noise from terrain-gen
- [ ] Entity rendering — draw entities on the map
- [ ] Physics/collision — simple grid or AABB
- [ ] Planning/AI — entity behaviors

## Later
- [ ] notcurses backend swap
- [ ] rayon parallelism where needed
- [ ] ratatui UI panels (inventory, stats, etc.)
