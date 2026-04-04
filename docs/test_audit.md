# Test Suite Audit

**Date:** 2026-04-01  
**Total tests:** ~752 across 25 files

---

## Module Counts

| Module | Count | Notes |
|--------|-------|-------|
| `src/ecs/tests.rs` | 230 | AI/behavior system |
| `src/game/mod.rs` | 107 | Game loop integration |
| `src/game/render.rs` | 61 | Rendering logic |
| `src/ecs/groups.rs` | 32 | Group detection |
| `src/tilemap.rs` | 74 | A*, terrain, seasonal overlays |
| `src/simulation/day_night.rs` | 21 | Lighting, calendar, seasons |
| `src/simulation/maps.rs` | 20 | Influence/threat/exploration |
| `src/pipe_water.rs` | 20 | Fluid sim |
| `src/simulation/wind.rs` | 16 | Wind/atmospheric moisture |
| `src/simulation/moisture.rs` | 16 | Moisture + 7 diagnostic tests |
| `src/pathfinding/flow_field.rs` | 18 | Flow field registry |
| `src/simulation/traffic.rs` | 14 | Traffic/road conversion |
| `src/game/events.rs` | 13 | Threat events |
| `src/simulation/scent.rs` | 11 | Scent diffusion |
| `src/game/chokepoint.rs` | 10 | Chokepoint detection |
| `src/pathfinding/graph.rs` | 12 | NavGraph |
| `src/pathfinding/region.rs` | 10 | Region pathfinding |
| `src/simulation/water_map.rs` | 9 | Water map |
| `src/game/dirty.rs` | 9 | Dirty tracking |
| `src/simulation/soil_fertility.rs` | 6 | Soil fertility |
| `src/terrain_pipeline.rs` | 17 | Pipeline output validation |
| `src/main.rs` | 20 | Pipeline/biome checks |
| `src/ecs/spatial.rs` | 17 | Spatial hash |
| `src/ecs/ai_arrays.rs` | 7 | AI batch arrays |
| `src/scripting/mod.rs` | 5 | Lua scripting |
| `src/simulation/vegetation.rs` | 2 | Vegetation growth |
| `tests/integration.rs` | 17 | Full game lifecycle |

---

## Categories

**Unit tests (tests one function/struct):** ~420 (56%)  
Pure function tests: `scent_map_emit_and_get`, `fertility_degrade_clamps_to_zero`, `traffic_map_accumulates`, `road_terrain_properties`, `stockpile_fullness_is_scarce`. These are the best tests in the codebase — tight inputs, tight assertions.

**Integration tests (multiple systems interact):** ~250 (33%)  
`wolf_hunts_and_kills_rabbit`, `villager_settlement_survival`, `villager_builds_at_site`, `full_ecosystem_simulation`, `game_over_when_all_villagers_die`, `traffic_converts_grass_to_road`. These tests call real systems without mocks — correct approach for a game sim.

**Smoke tests (just doesn't crash):** ~50 (7%)  
`game_survives_1000_ticks_without_panic`, `game_survives_5000_ticks_with_rain`, `traffic_overlay_renders_without_panic`, `water_animation_renders_without_panic`, `wind_at_map_edges_no_crash`. Valuable for regression but assert nothing about simulation correctness.

**Diagnostic tests (prints data, minimal assertions):** ~7 (1%)  
`diag_1_initial_state_after_worldgen_analog` through `diag_7_dual_moisture_system_conflict` in `simulation/moisture.rs`. See section below.

---

## Quality Issues

### RNG-dependent / Flaky

`full_ecosystem_simulation` (ecs/tests.rs:796) — runs 1000 ticks with no fixed seed, asserts nothing. It collects `states_seen` into a HashSet and prints it, but there are **zero assertions on that set**. This is a disguised diagnostic test that passes unconditionally. It would never catch a regression.

`wolf_hunts_and_kills_rabbit` (ecs/tests.rs:722) — runs 300 ticks and asserts the rabbit dies. Because the game uses internal RNG seeded off system time, this is technically flaky, though in practice the deterministic movement makes it stable. The bigger issue: if hunting broke, the loop would just time out and still fail cleanly. Acceptable risk.

`population_growth_spawns_villager` (game/mod.rs:3159) — contains the actual spawn logic **inline in the test** rather than calling a real game method. This tests a reimplemented version of population growth, not the actual `step()` loop. If the real logic changes, this test stays green.

### No assertions (pure noise)

- `full_ecosystem_simulation` — 75 lines, zero assertions, two `eprintln!` at end.
- `traffic_overlay_renders_without_panic` — calls `game.step()` once, asserts nothing. An identical test exists in integration.rs. Duplication.
- `particle_cap_at_max_particles` (game/mod.rs) should be checked — the name implies a numeric cap is enforced, but verify it doesn't just step without checking the count.

### Tests that test too much

`villager_settlement_survival` (ecs/tests.rs:1788) — 75 ticks with 11 spawned entities. When it fails, you get no information about *which* subsystem caused it. Should be split into smaller invariant tests.

`game_survives_5000_ticks_with_rain` — this runs for a measurable wall-clock time. 5000 ticks on a 60×42 map with the fluid sim running is slow. Not a hard problem, but it's the heaviest smoke test in the suite.

### Tests with weak assertions

`pipeline_generates_varied_biomes` (terrain_pipeline.rs) — asserts that at least 2 different biomes exist. This would pass even if the pipeline produced only two tiles of different terrain. Should assert a minimum count per expected biome (e.g., at least 5% forest on a seeded map).

`biome_distribution` — similar issue; checks biome counts exist but doesn't anchor them to expected geological distributions.

---

## Diagnostic Tests

The seven `diag_*` tests in `simulation/moisture.rs` (`diag_1` through `diag_7`) are extended setup + eprintln + minimal assertions. They were clearly written to debug the water cycle. Most have no assertions beyond "the code does not panic." They are **noise in CI** — they run slowly (diag_3 runs 500 ticks), print nothing visible unless `-- --nocapture` is passed, and cannot catch regressions.

**Recommendation:** Convert `diag_3_full_water_cycle_with_advect` and `diag_4_moisture_vegetation_chain` into real assertions (e.g., "after 500 ticks, inland moisture at x=40 should be >0.1"). Delete or gate the rest with `#[ignore]`.

---

## Coverage Gaps

**Fire spread AI response** — `villager_flees_from_fire` and fire ignition tests exist, but there are no tests for the fire containment threshold (does a tile act as firebreak? does road block spread?). `fire_does_not_spread_across_road` exists in game/mod.rs, which is good.

**Scripting integration** — Only 5 scripting tests, all unit-level. There are no tests verifying that a Lua hook can modify villager AI decisions. `lua_on_tick_updates_game_state` in game/mod.rs covers the hook callback, but not Lua-driven behavior changes.

**Particle system** — 7 tests exist but `particle_cap_at_max_particles` is the only cap boundary test. No test verifies particles clean up when a building is destroyed.

**Exploration / fog of war** — `exploration_expands_as_villagers_move` and `settlement_start_area_is_pre_revealed` exist. Gap: no test for fog-of-war rendering (overlay mode shows correct revealed state).

**NavGraph / region pathfinding under load** — region tests cover basic paths. No test for the dirty-region cap overflow under simultaneous terrain changes (e.g., a fire burning through a region).

**Seasonal overlay persistence** — `seasonal_cycle_does_not_corrupt_terrain` in integration.rs covers the happy path but not the case where ice/flood states survive a save-load cycle.

---

## Mocking Assessment

No mocks. This is correct for a game sim. All tests use real ECS, real tilemap, real systems. The closest thing to a mock is `HeadlessRenderer`, which is a legitimate null-output renderer, not a mock of game logic. `walkable_map()` helper in ecs/tests.rs is appropriate fixture construction.

---

## Most Valuable Tests

1. **`wolf_hunts_and_kills_rabbit`** — exercises sight, seek, movement, collision, eating, despawn in one test. Would catch most predator AI regressions.
2. **`astar_routes_through_ford` / `astar_prefers_roads`** (tilemap.rs) — specific terrain cost assertions. Would catch pathfinding weight regressions.
3. **`traffic_converts_grass_to_road`** (game/mod.rs) — tests the core roads-from-traffic mechanic end-to-end.
4. **`save_load_round_trip`** / `serialize_deserialize_world_round_trip` — serialization bugs are silent and destructive; these tests catch them.
5. **`scent_map_diffuse_spreads_to_neighbors`** — exact numeric assertion on diffusion physics; regression-proof.
6. **`flow_field_prefers_road_over_forest`** — verifies cost weighting in the flow field; would catch a terrain cost table change.

## Least Valuable Tests

1. **`full_ecosystem_simulation`** — no assertions, just eprintln. Should be deleted or converted.
2. **`diag_1` through `diag_7`** in moisture.rs — silent in CI, no assertions on most, slow.
3. **`population_growth_spawns_villager`** (game/mod.rs) — reimplements the logic it's supposed to test inline. Green even if the real `Game::step()` growth is broken.
4. **`traffic_overlay_renders_without_panic`** — duplicates coverage that integration smoke tests already provide.
5. **`pipeline_generates_varied_biomes`** — asserts `>= 2` biomes; would pass if the pipeline was almost completely broken.

---

## Summary

The suite is large and well-structured. Unit tests on simulation primitives (scent, traffic, wind, fertility) are high quality — precise inputs, precise assertions. The ECS behavioral tests are thorough but a handful lack assertions and would not catch regressions. The main gaps are: diagnostic tests that should be converted or culled, one test that reimplements production logic inline (`population_growth_spawns_villager`), and weak assertions on terrain pipeline output. No mocking issues.
