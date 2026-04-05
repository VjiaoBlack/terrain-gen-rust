# Implementation Backlog

Prioritized list of terrain/simulation features to implement. Each item references research docs with detailed algorithms.

## Priority 0: SimpleHydrology Port (REPLACES Priority 1 + 2)

### Port Nick McDonald's SimpleHydrology particle erosion system
- **Status**: IN PROGRESS
- **Why**: Replaces SPL erosion + hillslope diffusion + deposit sediment + river meandering with ONE unified system. Produces meandering rivers, proper deposition (deltas/floodplains), and realistic channel formation. The best terrain we've seen from any reference.
- **Source**: https://github.com/weigert/SimpleHydrology
- **Blog**: "Procedural Hydrology Improvements and Meandering Rivers" by Nick McDonald
- **Research**: `docs/research/soilmachine_deep_dive.md` (sections 3a-3d), `docs/research/nickmcd_meandering.md`

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
