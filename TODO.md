# terrain-gen-rust TODO

## Foundation [done]
- [x] Renderer trait + crossterm backend with double buffering
- [x] Game loop with configurable FPS
- [x] Input handling
- [x] Optional background color (None = terminal default)

## AI Harness [next]
- [ ] Headless renderer — no-op `Renderer` impl for max-speed sim and testing
- [ ] Frame serialization — serde snapshot of rendered frame for AI/subagent consumption
- [ ] Step mode — advance one tick at a time, return frame
- [ ] Programmatic input injection — feed actions via stdin or API
- [ ] Frame diffing — what changed between ticks
- [ ] Test harness — `cargo test` runs headless game scenarios

## Engine Core
- [ ] ECS — hecs or custom, entity/component storage
- [ ] Input abstraction — decouple from crossterm events so AI can inject inputs
- [ ] Terminal resize handling

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
