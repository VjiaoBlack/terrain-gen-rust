# Implementation Backlog

Prioritized list of terrain/simulation features to implement. Each item references research docs with detailed algorithms.

## Priority 1: Erosion System Improvements

### Deposition pass after SPL erosion
- **Status**: NOT STARTED
- **Why**: SPL is incision-only — cannot create deltas, floodplains, or alluvial fans. The paper itself says "depositional landforms need an additional deposition layer." Current workaround skips river mouth tiles but doesn't create proper depositional features.
- **Research**: `docs/research/analytical_erosion.md` (Known Limitations section), SoilMachine particle descent deposits sediment at end of particle life.
- **Approach**: After SPL pass, run a simple particle descent where sediment accumulates where slope < threshold. OR: compute sediment capacity from slope and deposit excess.

### Hillslope diffusion (Laplacian smoothing)
- **Status**: NOT STARTED
- **Why**: Prevents sharp erosion artifacts, simulates soil creep on slopes. The Tzathas 2024 paper includes this as a separate pass after SPL.
- **Research**: `docs/research/analytical_erosion.md` (section on hillslope diffusion add-on)
- **Approach**: Simple 4-neighbor Laplacian diffusion on heights, applied after SPL. ~30 lines of code. Only affects tiles above water_level.

### Coastal erosion root cause
- **Status**: PARTIALLY ADDRESSED (skip river mouths + reduced K + per-tile cap)
- **Why**: D8 flow routing concentrates all drainage at coast, creating artificially high erosion. Multi-pass helps but doesn't fix the fundamental issue.
- **Research**: `docs/research/analytical_erosion.md` (Known Limitations section)
- **Proper fix**: Deposition pass (above) would naturally build deltas at river mouths instead of eroding them.

## Priority 2: River Meandering (Highest Visual ROI)

### Momentum + discharge maps
- **Status**: NOT STARTED
- **Why**: Creates meandering rivers from straight channels. Highest ROI port from SoilMachine. Two float buffers per cell + exponential blend.
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 3c: Momentum Field Update), `docs/research/nickmcd_meandering.md`
- **Approach**: Add `momentum_x/y` and `discharge` fields. During erosion, particles accumulate momentum. Exponential blend (`lerp(old, new, 0.2)`) after each cycle. Particles get deflected by prior paths → self-reinforcing coherent channels.

## Priority 3: Vegetation-Erosion Coupling

### Root-density erosion resistance
- **Status**: NOT STARTED
- **Why**: Creates feedback loop: flat land → plants → roots resist erosion → stable soil. Channels → no plants → more erosion → deeper channels.
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 6: Vegetation-Erosion Coupling)
- **Approach**: `root_density` f32 per cell, updated by vegetation. Erosion scaled by `(1 - root_density)`. Plants die if discharge rises (flood).

## Priority 4: Groundwater Improvements

### Per-soil-type hydraulic conductivity
- **Status**: NOT STARTED (currently global K=0.015)
- **Why**: Sand: high K (fast drainage), clay: low K (waterlogged), rock: near zero. Gives realistic seepage patterns.
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 3d), groundwater research findings
- **Approach**: Map existing SoilType enum to K values. Use in Darcy's law diffusion step.

### Springs (water table surfaces)
- **Status**: NOT STARTED
- **Why**: Where water_table >= terrain_height, springs emerge. Replaces ad-hoc water placement.
- **Research**: groundwater research findings
- **Approach**: Track `water_table_depth` per tile. When hydraulic head exceeds terrain, add surface water.

## Priority 5: Atmosphere Improvements

### Moisture residence time
- **Status**: NOT STARTED
- **Why**: Real atmospheric moisture persists 4-10 days (~100-240 ticks). Current model precipitates too aggressively.
- **Research**: Nature Reviews 2021 (cited in timescale research), Nick McDonald procedural weather
- **Approach**: Exponential decay: `precip_rate = moisture * (1 - e^(-dt/tau))` where tau ~ 150 ticks.

## Future / Low Priority

### Soil column LayerMap (stratigraphy)
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 1: LayerMap)
- High complexity. Linked list of soil layers per cell. Would enable exposed rock faces, geological stratigraphy.

### Sediment type conversion graph
- **Research**: `docs/research/soilmachine_deep_dive.md` (section 2: Sediment Conversion Graph)
- Rock → gravel → sand → soil chain. Each erosion step produces the `erodes_to` type.
