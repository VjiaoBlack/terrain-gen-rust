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
- `src/hydrology.rs` — particle descent, momentum, cascade, discharge tracking
- Default erosion model in pipeline (ErosionModel::SimpleHydrology)

### Phase 2: Render rivers from discharge field (NEXT — QUICK WIN)
- **Status**: NOT STARTED
- **Why**: We have the discharge data but don't render it. Nick's system renders rivers as `erf(0.4 * discharge)` blended onto terrain color — no water tiles needed.
- **Approach**: In tile rendering, blend terrain color toward blue-gray `(92, 133, 142)` based on `erf(0.4 * discharge)`. Add specular boost for water look.
- **Research**: `docs/research/meandering_rivers_2023.md` (River Rendering section)

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

## Priority 1: Bug Fixes

### Ocean vs pipe_water rendering mismatch
- **Status**: NOT FIXED
- **Why**: Static `Terrain::Water` tiles and dynamic `pipe_water` depth tiles render differently in the ocean. Visible as odd single tiles with different lighting.
- **Approach**: Unify the water rendering path — both should use the same color/lighting logic.

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
