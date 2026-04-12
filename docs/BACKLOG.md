# Implementation Backlog

Prioritized list of terrain/simulation features to implement. Each item references research docs with detailed algorithms.

**NEW AGENTS: Start here.** Read this file, then read the design docs linked below before picking up work.

---

## Priority 0A: Agent Evaluation Infrastructure ⭐ NEW

**Status:** DESIGN COMPLETE — ready for implementation
**Design doc:** `docs/design/cross_cutting/agent_evaluation_infrastructure.md`
**Depends on:** `automated_visual_qa.md`, `terrain_test_harness.md`

Build the infrastructure that lets AI agents *see* and *evaluate* the game, enabling autonomous improvement loops.

### Phase 1: Capture Pipeline (NEXT)
- [ ] `scripts/capture_eval_frames.sh` — render screenshots as PNGs for 3 seeds at 4 timepoints
- [ ] `--dump-state` CLI flag — JSON state dump (population, resources, terrain stats, simulation health)
- [ ] `tests/baselines/` — golden seed metric files (seeds 42, 137, 777)
- [ ] `docs/metrics_history.json` — append-only trend log

### Phase 2: Evaluation Pipeline
- [ ] `generate_report_card()` in Rust — TITAN-style perception abstraction (narrative status, not raw numbers)
- [ ] Evaluation prompt template using rubric from design doc
- [ ] Integrate rubric scoring into daily health check agent
- [ ] Regression detection against golden seed baselines

### Phase 3: Gamedev Agent Loop
- [ ] Scheduled gamedev agent that reads health report + eval scores
- [ ] Agent picks lowest-scoring rubric category, writes sprint contract, implements
- [ ] Commits to branch only if rubric score improved or held steady

### Phase 4: Self-Improving Evaluation
- [ ] When human catches something rubric missed → add rubric item
- [ ] Track rubric scores in metrics_history — evaluation itself improves over time

**Why this matters:** Without this, agents can fix bugs but can't tell if the game is getting *better*. This closes the qualitative feedback loop.

---

## Priority 0B: Terrain Test Harness

**Status:** DESIGN COMPLETE — partially implemented
**Design doc:** `docs/design/cross_cutting/terrain_test_harness.md`
**Also see:** `docs/design/cross_cutting/automated_visual_qa.md`

Automated tests that catch what a human catches in 2 seconds of looking at the screen.

- [ ] `pipeline_health` test (biome diversity, river coverage, coastal artifacts, height distribution)
- [ ] Pre-commit hook blocking broken terrain changes
- [ ] PostEdit Claude Code hook for terrain/render files
- [ ] Missing diagnostic overlays (Discharge, Slope, Soil, Biome, Light Map)
- [ ] Visual snapshot regression test (known-good seed stats)

---

## Priority 0C: Hydrology System (SimpleHydrology → soillib upgrades)

### Phase 1: SimpleHydrology base port ✅ DONE
- `src/hydrology.rs` — faithful line-by-line port of Nick's C++ (water.h, world.h)
- Erosion-first pipeline: normalized [0,1] terrain → erosion shapes it → biomes after
- Runtime erosion every 100 ticks (geological pace)
- `--live-gen` mode shows erosion in real time, passes heightmap to game

### Phase 2: Render rivers from discharge field ✅ DONE
- Discharge → `erf(0.4 * d)` → water color blend in both Normal and Landscape modes
- Discharge seeds pipe_water depth in strong channels (erf > 0.5)
- River rendering skipped on Terrain::Water tiles

### Phase 2.5: Water System Unification ⭐ BLOCKED (do after 0D)
- **Status**: DESIGNED — waiting for state-driven architecture refactor
- **Design doc**: `docs/design/cross_cutting/state_driven_architecture.md`
- **Why**: 3 water systems (Terrain::Water, pipe_water, discharge) don't talk to each other
- **Plan**: See Priority 0D below

### Phase 3: Upgrade to soillib algorithms
- **Status**: NOT STARTED
- **Why**: soillib (2023) has significant improvements over SimpleHydrology that produce better meandering and more physical results.
- **Research**: `docs/research/meandering_rivers_2023.md`
- **Source**: https://github.com/erosiv/soillib

**Upgrades in priority order:**
1. **Viscosity-based implicit Euler** — replace dot-product momentum transfer with `speed = 1/(1+ds*(bedShear+viscosity)) * speed + ds*viscosity/(1+ds*(bedShear+viscosity)) * avg_speed`. Key to meandering quality.
2. **Stream power law sediment** — `suspend = ks * vol * slope * discharge^0.4` with separate suspend/deposit rates. Replaces linear `c_eq`.
3. **Separate sediment buffer** — bedrock vs loose sediment. Erosion removes sediment first.
4. **5-point gradient stencil** — 4th-order accurate: `(f[-2] - 8*f[-1] + 8*f[+1] - f[+2]) / 12`
5. **Dynamic timestep** — `ds = cell_distance / speed` per step (not fixed sqrt(2))
6. **Debris flow** — path-integral thermal erosion replacing 8-neighbor cascade

**What to implement (~350 lines in `src/hydrology.rs`):**
1. `HydroCell` — 8 floats: height, discharge, momentum_x/y, tracking buffers, root_density
2. `Drop::descend()` — particle descent with momentum transfer, fixed-step `sqrt(2)*cellsize`, sediment equilibrium with `erf(0.4 * discharge)` squash
3. `erode(cycles)` — clear tracking, spawn particles, run to completion, exponential-blend tracking→persistent (`lrate=0.1`)
4. `cascade(pos)` — 8-neighbor talus relaxation, runs inside each particle step (not just at end)
5. Optional: vegetation root_density coupling

**Key parameters (defaults):**
- `evap_rate=0.001, deposition_rate=0.1, min_vol=0.01, max_age=500`
- `entrainment=10.0, gravity=1.0, momentum_transfer=1.0`
- `lrate=0.1, max_diff=0.01, settling=0.8`

**Gotchas from source code (vs our docs):**
- Speed normalized to `sqrt(2) * cellsize` every step — fixed step, not variable
- Cascade runs INSIDE each particle step, not after all particles
- `c_eq` uses `erf(0.4 * discharge)` sigmoid squash, not raw discharge
- Momentum used directly in force formula with `/ (volume + discharge)`, not divided separately

**What it replaces (feature-flag out, don't delete):**
- `analytical_erosion.rs` (SPL erosion)
- `hillslope_diffusion()` in terrain_pipeline.rs
- `deposit_sediment()` in terrain_pipeline.rs
- Skip-river-mouths hack in SPL
- Multi-pass SPL iteration

**Integration**: New `ErosionModel` enum in PipelineConfig (SPL vs SimpleHydrology), default to SimpleHydrology. Wire into `run_pipeline()`.

## Priority 0D: State-Driven Architecture Refactor ⭐ AFTER EVAL INFRA

**Status:** DESIGNED — do after Priority 0A (eval infra) and 0B (test harness) so we can verify the refactor doesn't break things.
**Design doc:** `docs/design/cross_cutting/state_driven_architecture.md`
**Depends on:** 0A (eval infra), 0B (test harness)

Refactor the simulation to follow state-driven principles: single source of truth, derived data as pure functions, systems as only writers.

### Stage 1: Unify water rendering ✅ DONE
- [x] `water_visual()` in shared.rs — ONE function for all water
- [x] Normal + Landscape modes use unified path
- [x] Ocean, rivers, rain, floods all render identically

### Stage 2: Make Terrain::Water derived ✅ DONE
- [x] Every 20 ticks, reclassify tiles from pipe_water depth
- [x] Flooded tiles become Water, dried tiles revert to biome
- [x] Walkability/pathfinding/ice respond to dynamic water automatically

### Stage 3: Canonical WorldState struct ✅ DONE
- [x] `src/world_state.rs` — defines target struct (not yet wired into Game)
- [x] Removed duplicate `discharge` field (was copy of `hydro.discharge`)
- [x] All code reads `hydro.discharge` directly — single source of truth

### Stage 4: Full migration to WorldState (FUTURE)
- [ ] Nick's discharge field = where rivers ARE (locations)
- [ ] pipe_water = actual water depth in those channels (from rain, groundwater)
- [ ] Ocean = boundary condition on pipe_water (constant inflow at map edges)
- [ ] Remove Terrain::Water enum variant entirely

### Why wait for eval infra first:
This refactor touches rendering, walkability, pathfinding, ice, floods, biome classification — basically everything. Without the test harness and eval pipeline catching regressions, we'll ship broken terrain and not notice (like we've been doing).

---

## Priority 1: Bug Fixes

(Ocean rendering mismatch moved to Priority 0D Stage 1)

### Simulation non-determinism across same-seed replays
- **Status**: NOT STARTED
- **Why**: Health check agent reports seed 137 producing food=3 on one run and food=15 on another at the same tick count. Same-seed replays should be deterministic. Likely `thread_rng()` leaking into breeding/death systems.
- **Approach**: Grep for `thread_rng()` in game loop code paths. Replace with seeded RNG passed through `Game` struct. Verify with a test that two same-seed runs produce identical state.

## Priority 2: Vegetation-Erosion Coupling

### Root-density erosion resistance
- **Status**: NOT STARTED
- **Why**: Creates feedback loop: flat land → plants → roots resist erosion → stable soil. Channels → no plants → more erosion → deeper channels.
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 6: Vegetation-Erosion Coupling)
- **Approach**: Part of SimpleHydrology port — `root_density` float per cell, erosion scaled by `(1 - root_density)`.

## Priority 3: Groundwater Improvements

### Per-soil-type hydraulic conductivity
- **Status**: NOT STARTED (currently global K=0.015)
- **Why**: Sand: high K (fast drainage), clay: low K (waterlogged), rock: near zero.
- **Approach**: Map existing SoilType enum to K values in Darcy's law diffusion step.

### Springs (water table surfaces)
- **Status**: NOT STARTED
- **Why**: Where water_table >= terrain_height, springs emerge.
- **Approach**: Track `water_table_depth` per tile. When hydraulic head exceeds terrain, add surface water.

## Priority 4: Atmosphere Improvements

### Moisture residence time
- **Status**: NOT STARTED
- **Why**: Real atmospheric moisture persists 4-10 days (~100-240 ticks). Current model precipitates too aggressively.
- **Approach**: Exponential decay: `precip_rate = moisture * (1 - e^(-dt/tau))` where tau ~ 150 ticks.

## Future / Low Priority

### Soil column LayerMap (stratigraphy)
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 1: LayerMap)
- High complexity. Linked list of soil layers per cell.

### Sediment type conversion graph
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 2: Sediment Conversion Graph)
- Rock → gravel → sand → soil chain.
