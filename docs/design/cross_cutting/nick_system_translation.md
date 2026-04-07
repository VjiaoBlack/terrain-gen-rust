# Nick's SimpleHydrology → Our System: Line-by-Line Translation Guide

**Goal:** Make our terrain generation produce results visually identical to Nick's SimpleHydrology. Not "inspired by" — the SAME system, translated to Rust, rendering in a terminal instead of OpenGL.

---

## Nick's Files → Our Files

| Nick's file | Lines | Our file | What to do |
|---|---|---|---|
| `cellpool.h` (cell struct) | ~20 | `hydrology.rs` (HydroMap) | ✅ Done — 8 floats per cell |
| `cellpool.h` (map::init) | ~90 | `terrain_gen.rs` + `terrain_pipeline.rs` | ❌ CHANGE: 8 octaves, normalize [0,1], no water_level threshold |
| `water.h` (Drop::descend) | ~100 | `hydrology.rs` (Drop::descend) | ✅ Mostly done — verify line by line |
| `world.h` (erode) | ~30 | `hydrology.rs` (erode) | ✅ Done |
| `world.h` (cascade) | ~80 | `hydrology.rs` (cascade) | ⚠️ Verify: underwater behavior, bidirectional transfer |
| `vegetation.h` | ~190 | `simulation/vegetation.rs` | ❌ NOT DONE: root density feedback into erosion |
| `SimpleHydrology.cpp` (main loop) | ~50 | `terrain_pipeline.rs` (run_pipeline) | ❌ CHANGE: pipeline ordering, cycle count |
| `shader/default.vs` (color) | ~15 | `render/landscape.rs` | ⚠️ Partial: discharge blend done, steepness blend NOT done |
| `shader/image.fs` (lighting) | ~30 | `simulation/day_night.rs` | ✅ Blinn-Phong done, specular boost from discharge NOT done |

---

## Step-by-Step Translation

### Step 1: Terrain Initialization

**Nick's `map::init()` (cellpool.h:323-411):**
```cpp
FastNoiseLite noise;
noise.SetNoiseType(FastNoiseLite::NoiseType_OpenSimplex2);
noise.SetFractalType(FastNoiseLite::FractalType_FBm);
noise.SetFractalOctaves(8);
noise.SetFractalLacunarity(2.0);
noise.SetFractalGain(0.6);
noise.SetFrequency(1.0);
noise.SetSeed(SEED);

// Generate
float min = 1e6, max = -1e6;
for each cell:
    h = noise.GetNoise(x * 80.0, y * 80.0)  // mapscale=80
    track min/max

// Normalize to [0,1]
for each cell:
    h = (h - min) / (max - min)
```

**Our equivalent (what to write):**
```rust
fn generate_normalized_terrain(w: usize, h: usize, seed: u32) -> Vec<f64> {
    let perlin = Perlin::new(seed);
    let mut heights = vec![0.0; w * h];
    let scale = 80.0 / 512.0; // Nick uses mapscale=80 on 512 grid

    // 8 octaves, lacunarity=2.0, gain=0.6 (Nick's exact params)
    for y in 0..h {
        for x in 0..w {
            let mut freq = 1.0;
            let mut amp = 1.0;
            let mut val = 0.0;
            for _ in 0..8 {
                val += amp * perlin.get([
                    x as f64 * scale * freq,
                    y as f64 * scale * freq,
                ]);
                freq *= 2.0;
                amp *= 0.6;
            }
            heights[y * w + x] = val;
        }
    }

    // Normalize to [0,1]
    let min_h = heights.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_h = heights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = (max_h - min_h).max(1e-10);
    for h in &mut heights {
        *h = (*h - min_h) / range;
    }
    heights
}
```

**Current state:** We use `terrain_gen::generate_terrain()` which:
- Uses 4 octaves (not 8)
- Does NOT normalize to [0,1]
- Applies water_level as a generation parameter
- Returns a TileMap with classified terrain types

**Action:** Add `generate_normalized_terrain()` to `terrain_pipeline.rs`. Use it when `ErosionModel::SimpleHydrology`.

### Step 2: Erosion (the shaping pass)

**Nick's main loop (SimpleHydrology.cpp:314-356):**
```cpp
// Called every frame:
world.erode(quad::tilesize);  // 512 particles
Vegetation::grow();
// update rendering...
```

He runs `erode(512)` every frame for the entire runtime. The terrain evolves continuously. For our worldgen, we need to simulate "hundreds of frames" at startup.

**Our equivalent:**
```rust
// In run_pipeline, when SimpleHydrology:
let heights = generate_normalized_terrain(w, h, seed);
let mut hydro = HydroMap::new(w, h);
let params = HydroParams::default(); // Nick's exact params

// Simulate ~500 "frames" of erosion
for cycle in 0..500 {
    erode(&mut heights, &mut hydro, &params, 512, seed + cycle);
    // Optional: vegetation growth every N cycles
}
// Water level is now just a render threshold
let water_level = 0.1; // lowest 10% = ocean (Nick uses 0.1)
```

**Current state:** We run 200 cycles with scaled particles, but on terrain that was already processed (thermal erosion, priority flood, 4 octaves).

**Action:** Replace the entire SimpleHydrology pipeline path with: normalized noise → erode 500 cycles → classify biomes.

### Step 3: Rendering Color

**Nick's vertex shader (default.vs:50-59):**
```glsl
float steepness = 1.0 - pow(clamp((normal.y - 0.4) / 0.6, 0, 1), 2);
color = mix(flatColor, steepColor, steepness * steepness);
if (steepness > 0.6) color = steepColor;
color = mix(color, waterColor, discharge);
```

**Translation:**
```rust
// In landscape_terrain_glyph:
let normal_y = /* z component of surface normal, [0,1] range */;
let steepness = 1.0 - ((normal_y - 0.4) / 0.6).clamp(0.0, 1.0).powi(2);
let flat_color = Color(50, 81, 33);   // Nick's flatColor
let steep_color = Color(115, 115, 95); // Nick's steepColor
let water_color = Color(92, 133, 142); // Nick's waterColor

// Slope blend
let base = blend(flat_color, steep_color, steepness * steepness);
// Discharge blend
let discharge_alpha = erf(0.4 * discharge);
let final_color = blend(base, water_color, discharge_alpha);
```

**Current state:** We blend soil color + vegetation color based on biome type, then add discharge. We DON'T have steepness-based color blending.

**Action:** Add steepness blend BEFORE discharge blend in landscape rendering.

### Step 4: Cascade (thermal erosion)

**Nick's cascade (world.h:90-168):**
```cpp
void cascade(vec2 pos) {
    auto get = [&](auto& c) { return c.height; };
    // Sort 8-neighbors ascending by height
    // For each: excess = |diff| - dist * maxdiff * lodsize (above sea)
    //           excess = |diff| (below sea level 0.1)
    // Transfer = settling * excess / 2
    // Bidirectional: high→low or low→high
}
```

**Critical detail we might be getting wrong:** Nick sorts neighbors by height ascending and processes them in order. Our cascade iterates in fixed direction order. This matters because transferring to one neighbor changes the height for subsequent neighbor checks.

**Action:** Verify cascade matches Nick's exactly — sort neighbors, process in height order.

### Step 5: Vegetation Root Density Feedback

**Nick's vegetation (vegetation.h):**
```cpp
struct Plant { vec2 pos; float size; };
// Plants write root density to 3x3 kernel:
// center: 1.0, cardinal: 0.6, diagonal: 0.4
// Root density scales deposition rate: effD = depositionRate * (1 - rootdensity)
```

**Current state:** We have VegetationMap with per-tile vegetation level, but it does NOT write to hydro.root_density. The root_density field exists in HydroMap but is always 0.0.

**Action:** Before each erosion cycle block, populate `hydro.root_density` from `vegetation.get(x, y)`. Or, during the 500-cycle worldgen erosion, periodically run a simplified vegetation step that sets root density from the current height/slope/discharge.

---

## The Complete Pipeline (what run_pipeline should look like)

```rust
ErosionModel::SimpleHydrology => {
    // 1. Generate normalized terrain (Nick's exact noise params)
    let mut heights = generate_normalized_terrain(w, h, seed);
    let water_level = 0.1; // 10% ocean

    // 2. Run erosion as THE terrain shaper (not a post-process)
    let params = HydroParams {
        water_level,
        ..HydroParams::default() // Nick's exact values
    };
    let mut hydro = HydroMap::new(w, h);
    for cycle in 0..500 {
        erode(&mut heights, &mut hydro, &params, 512, seed + cycle);
    }

    // 3. NOW classify biomes from the eroded heightmap
    let slope = compute_slope(&heights, w, h);
    let temperature = compute_temperature(&heights, w, h, seed);
    // Use discharge as moisture proxy near rivers
    let moisture = compute_moisture_from_discharge(&heights, &hydro.discharge, w, h, water_level);
    for each tile:
        biome = classify_biome(height, temp, moisture, slope, water_level);

    // 4. Soil assignment from eroded terrain
    let soil = assign_soil(&heights, &slope, &moisture, &river_mask_from_discharge, ...);

    // 5. Store discharge for river rendering
    discharge = hydro.discharge;
}
```

---

## Verification Checklist

After implementation, these should all be true:

- [ ] Terrain starts as [0,1] normalized noise, 8 octaves
- [ ] No priority flood, no thermal erosion pre-pass
- [ ] Erosion runs 500 cycles × 512 particles before biome classification
- [ ] Water level = 0.1 (not 0.42)
- [ ] Discharge field has clear river channels (top 1% tiles are 10x+ higher than average)
- [ ] River channels are geometrically lower than surrounding terrain (carved valleys)
- [ ] Landscape rendering uses steepness blend (flat green → steep grey) + discharge blend (→ blue)
- [ ] No single biome covers > 50% of land
- [ ] Visible river tiles = 2-10% of land area
- [ ] Generation completes in < 10 seconds on 256x256
