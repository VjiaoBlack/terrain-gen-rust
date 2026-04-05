# Integrating Nick McDonald's SimpleHydrology

**Status:** Design — not yet implemented
**Source:** https://github.com/weigert/SimpleHydrology
**Blog:** https://nickmcd.me/2023/12/12/meandering-rivers-in-particle-based-hydraulic-erosion-simulations/

---

## The Problem

We tried to bolt Nick's erosion onto our existing terrain pipeline as a post-process. This doesn't work because:

1. **Priority flood destroys valleys** before erosion runs — removing exactly the features rivers need
2. **Our terrain gen creates discrete biomes first**, then erosion runs on classified terrain — Nick's system shapes terrain first, then color comes from slope + discharge
3. **Our rendering is biome-driven** (Grass/Forest/Desert → character + color) — Nick's is continuous (slope blend + discharge blend)
4. **Parameter tuning is futile** when the architecture is wrong — we spent hours tweaking particle counts, erf scales, normalization when the real issue is pipeline ordering

## How Nick's System Actually Works

His FULL pipeline (from `SimpleHydrology.cpp` + `cellpool.h`):

```
1. Generate heights: 8-octave FBm OpenSimplex2, normalize to [0,1]
   - NO biome classification
   - NO priority flood
   - NO thermal erosion pre-pass
   - Just raw normalized noise

2. Run erosion (every frame, continuously):
   - 512 particles per call
   - Each particle: gravity + momentum transfer → move → erode/deposit → cascade
   - Discharge and momentum fields accumulate via EMA (lrate=0.1)
   - Cascade runs INSIDE each particle step

3. Grow vegetation (every frame):
   - Plants spawn where slope < 0.8, discharge < 0.3, height < 0.8
   - Root density written to 3x3 kernel around each plant
   - Root density reduces erosion deposition rate

4. Render (every frame):
   - Color = mix(flatColor, steepColor, steepness²)
   - Color = mix(color, waterColor, erf(0.4 * discharge))
   - Trees = instanced cones at plant positions
   - Lighting = deferred Blinn-Phong + shadow map + SSAO
```

The KEY insight: **erosion IS the terrain shaper.** The noise is just a starting point. After hundreds of erosion cycles, the noise is barely recognizable — it's been carved into valleys, ridges, and floodplains by the water particles. Biomes don't exist — color comes from slope (steep = rocky) and discharge (wet = blue).

## What Our System Needs to Change

### Current pipeline (broken):
```
noise → thermal erosion → priority flood → hydrology (post-process) → biome classification → soil assignment → rendering by biome type
```

### Correct pipeline:
```
noise (8 octaves, normalized [0,1]) → hydrology (MANY cycles, primary shaper) → classify biomes from result → soil from slope+discharge → render with discharge rivers
```

### Specific changes:

**1. Terrain generation (terrain_gen.rs):**
- Increase to 8 octaves
- Normalize output to [0,1] range (min/max normalization, not water_level threshold)
- Water level is just a render threshold, not a terrain gen parameter

**2. Pipeline ordering (terrain_pipeline.rs):**
- Remove priority flood for SimpleHydrology path (DONE — but not enough)
- Remove thermal erosion pre-pass (hydrology cascade handles this)
- Run hydrology as THE primary terrain shaper, not a post-process
- Run MANY more cycles (500+) with proper particle density
- THEN classify biomes from the eroded heightmap + discharge field

**3. Biome classification:**
- Currently uses pipeline moisture (rainfall + water proximity) — should also use discharge
- High discharge = river/marsh biome
- Steep slope = cliff/rocky
- This replaces the current slope+temp+moisture Whittaker classifier for terrain near rivers

**4. Rendering:**
- Discharge-based river coloring works (now in both Normal and Landscape modes)
- But needs to be STRONGER — Nick's alpha goes up to 1.0 (pure water color)
- Current cap at 0.9 should be 1.0 for high-discharge channels
- Terrain color should also blend with steepness (steep = grey/brown rock) like Nick's

**5. Vegetation feedback:**
- Nick's plants write root_density which reduces erosion
- We have VegetationMap but it doesn't feed back into erosion
- Need to populate root_density from our vegetation data before/during erosion

## Implementation Plan

### Phase 1: Fix pipeline ordering
- In `run_pipeline`, when `ErosionModel::SimpleHydrology`:
  - Skip thermal erosion AND priority flood
  - Run hydrology with 500 cycles, 512 particles (matching Nick exactly)
  - Use the eroded heightmap for biome classification
  - Pass discharge field through to rendering

### Phase 2: Normalize terrain generation
- Add a normalization step after noise generation: `h = (h - min) / (max - min)`
- Water level becomes a fraction (e.g. 0.1 = lowest 10% is ocean)
- This gives the full [0,1] range for erosion to work with

### Phase 3: Steepness-based terrain coloring
- Add slope-based color blending in landscape rendering
- `color = mix(flatColor, steepColor, steepness²)` before discharge blend
- flatColor = green/brown (vegetation), steepColor = grey (rock)
- This is how Nick gets the "rocky mountain" look

### Phase 4: Vegetation feedback loop
- During hydrology cycles, periodically update root_density from vegetation
- Start with simple: root_density = vegetation_level at each cell
- This creates the stabilization feedback (plants protect soil)

### Phase 5: Runtime erosion (stretch goal)
- Run erosion per-tick during gameplay (like Nick does per-frame)
- Rivers slowly evolve, meander, shift course over game-time
- Discharge field updates live, rendering reflects changes

## Parameters (from Nick's actual code)

All of these should be used exactly as-is until we have a reason to change them:

```rust
// Drop parameters
evap_rate: 0.001,
deposition_rate: 0.1,
min_vol: 0.01,
max_age: 500,
entrainment: 10.0,
gravity: 1.0,
momentum_transfer: 1.0,

// World parameters
lrate: 0.1,        // EMA blend rate
max_diff: 0.01,    // cascade slope threshold
settling: 0.8,     // cascade transfer fraction

// Rendering
water_color: (92, 133, 142),
flat_color: (50, 81, 33),
steep_color: (115, 115, 95),
discharge_erf_scale: 0.4,
```

## What NOT to Change

- Don't normalize discharge — `erf(0.4 * raw)` is the intended mapping
- Don't add priority flood — hydrology cascade handles depression filling
- Don't pre-classify biomes before erosion — let erosion shape the terrain first
- Don't cap river alpha at 0.9 — let it go to 1.0 for real rivers
- Don't invent new parameter values — use Nick's until proven wrong with data
