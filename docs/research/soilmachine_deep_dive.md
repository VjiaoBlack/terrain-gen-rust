# SoilMachine Deep Dive — Nick McDonald's Terrain Simulation

Sources: SoilMachine blog post (2022-04-15), Meandering Rivers blog post (2023-12-12),
Procedural Weather blog post (2018-07-10), soillib GitHub (erosiv/soillib),
SimpleHydrology GitHub (weigert/SimpleHydrology).

---

## 1. LayerMap — Run-Length Encoded Soil Columns

The central data structure in SoilMachine is a 2D grid where each cell holds a
**doubly-linked list of soil sections** rather than a fixed-depth voxel stack.

```rust
// Conceptual Rust equivalent
struct Section {
    soil_type: u8,   // index into SoilParams table
    size: f32,       // layer thickness
    floor: f32,      // cumulative base height (redundant but cached)
    saturation: f32, // water content [0, 1]
    prev: Option<Box<Section>>,
    next: Option<Box<Section>>,
}

struct LayerMap {
    // 2D array of column heads
    columns: Vec<Option<Box<Section>>>,  // len = width * height
    pool: SectionPool,                   // pre-allocated slab to avoid fragmentation
}
```

**Why linked lists instead of a heightmap float?**
- Preserves geological stratigraphy: bedrock under gravel under sand under soil.
- `remove(amount)` from the top can return "unconverted thickness" when the removed
  layer exposes a different soil type underneath — triggering type-change cascades.
- Memory scales with actual layer count, not grid volume (no empty voxels).
- A fixed-size pool/slab allocator prevents heap fragmentation during continuous
  erosion-deposition cycles.

---

## 2. Sediment Conversion Graph

There is no explicit graph object. Conversions are embedded in per-type parameter structs:

```rust
struct SoilParams {
    // Hydraulic
    solubility: f32,        // how easily water dissolves this type
    erodes_to: u8,          // target type after hydraulic erosion (rock -> gravel)
    erosion_rate: f32,

    // Wind
    abrasion_rate: f32,     // wind breaks this into smaller type
    suspension_rate: f32,   // wind picks this up

    // Cascading / talus
    max_slope_angle: f32,   // degrees; stable angle of repose
    settling_velocity: f32,
    cascades_to: u8,        // type produced when cascading

    // Hydrology
    porosity: f32,          // fraction of volume that holds water
    density: f32,
}
```

Implicit conversion chain:
```
bedrock --[hydraulic erosion]--> gravel --[hydraulic erosion]--> sand --[hydraulic erosion]--> soil
   ^                                                                                              |
   |__________________________[compaction / time, not simulated]__________________________________|

gravel --[wind abrasion]--> sand --[wind abrasion]--> silt/dust
```

Each erosion step produces the `erodes_to` type at the deposition site.
The eroded-from site loses the top layer, potentially exposing a different type below.

---

## 3. Key Algorithms

### 3a. Sediment Cascading (Talus / Angle of Repose)

A cellular automaton that fires after any deposition event:

```
fn cascade(pos, layer_map, params):
    neighbors = 8-connected neighbors sorted by (height[n] - height[pos]) descending
    for n in neighbors:
        diff = height[pos] - height[n]
        soil = top_layer(pos)
        max_diff = params[soil.type].max_slope_angle_as_height_diff
        if diff > max_diff:
            transfer = params[soil.type].settling_velocity * (diff - max_diff) / 2.0
            unconverted = remove(pos, transfer)   // returns leftover if layer ran out
            add(n, transfer - unconverted, params[soil.type].cascades_to)
            if unconverted > 0:
                cascade(pos, ...)  // newly exposed layer may also be unstable
```

The `remove()` return value is the key: it triggers recursive cascade when a layer
boundary is crossed, propagating type changes down the column.

### 3b. Water Particle Descent (SimpleHydrology / soillib)

```
fn descend(drop):
    while drop.age < MAX_AGE and drop.volume > MIN_VOL:
        ipos = floor(drop.pos)
        n = surface_normal_finite_diff(ipos)  // NOT mesh normal — avoids saddle ambiguity

        // Gravity
        drop.speed += gravity * vec2(n.x, n.z) / drop.volume

        // Momentum transfer from accumulated stream field
        fspeed = momentum_map[ipos] / (discharge_map[ipos] + epsilon)
        if dot(fspeed, drop.speed) > 0:
            drop.speed += momentum_transfer * dot(norm(fspeed), norm(drop.speed))
                          / (drop.volume + discharge_map[ipos]) * fspeed

        // Variable timestep: exactly one cell per step
        drop.speed = normalize(drop.speed) * cell_diagonal

        drop.pos += drop.speed

        // Accumulate estimates (atomic in GPU version)
        discharge_track[ipos] += drop.volume
        momentum_track[ipos]  += drop.volume * drop.speed

        // Sediment equilibrium
        h_diff = height[ipos] - height[new_pos]
        c_eq = (1 + entrainment * discharge_map[ipos]) * max(0, h_diff)
        c_diff = c_eq - drop.sediment
        drop.sediment     += deposition_rate * (1 - root_density[ipos]) * c_diff
        height[ipos]      -= deposition_rate * (1 - root_density[ipos]) * c_diff

        drop.volume   *= (1 - evap_rate)
        drop.sediment /= (1 - evap_rate)  // mass-conservative: concentration rises

    height[ipos] += drop.sediment  // deposit remainder on death
    cascade(drop.pos)
```

### 3c. Momentum Field Update (Meandering)

After all N particles complete their trajectories each cycle:

```
// Exponential filter smooths noisy per-step accumulation into stable field
discharge[i] = lerp(discharge[i], discharge_track[i], lrate)  // lrate ~0.2
momentum[i]  = lerp(momentum[i],  momentum_track[i],  lrate)
discharge_track[i] = 0
momentum_track[i]  = vec2(0)
```

This two-buffer pattern (track vs. committed) is what makes meandering emerge:
particles are deflected by prior particle paths, self-reinforcing coherent channels.

### 3d. Water Seepage (Subsurface Flow)

```
fn seep(column, dt):
    for each section s from top down:
        available = s.saturation * s.size * params[s.type].porosity
        if s.next exists:
            capacity = (1 - s.next.saturation) * s.next.size * params[s.next.type].porosity
            transfer = min(available, capacity)
            s.saturation          -= transfer / (s.size * porosity)
            s.next.saturation     += transfer / (s.next.size * next_porosity)
```

Saturation reaching 1.0 in a layer forces water to surface (spring / waterlogging).

---

## 4. Weather System (2018 Prototype)

Five coupled scalar fields updated each simulated day:
- `height`, `wind_speed`, `temperature`, `humidity`, `precipitation`

```
// Each timestep:
wind = perlin_noise(time) * base_wind_speed
wind_speed[pos] *= slope_factor  // faster uphill, slower downhill
temperature[pos] = solar_base - altitude_lapse * height[pos]
humidity[pos] += evaporation_rate * temperature[pos]  // bodies of water source
humidity = advect(humidity, wind)
humidity = diffuse(humidity, 0.1)  // smooth after advection
if humidity[pos] > threshold and temperature[pos] < threshold:
    rain[pos] = true
    humidity[pos] -= rain_amount
```

Over 365 simulated days, averaged fields yield biome distribution maps
(rainfall, temperature, sunshine hours) — a cheap climate model.

---

## 5. soillib Architecture (GPU Version)

soillib is the production successor to SoilMachine: C++23 + CUDA, Python bindings.

Key structural changes vs. the blog-post version:
- Particles are CUDA kernel threads (one particle per thread, massively parallel).
- `model_t` stores flat GPU buffers: `height`, `sediment`, `discharge`,
  `momentum` (all `buffer_t<float>` or `buffer_t<vec2>`).
- `atomicAdd` replaces the sequential accumulation loop.
- `curandState` per-particle gives independent random spawn positions.
- Separate `erosion_thermal.cu` handles angle-of-repose cascading on GPU.
- Typed via C++23 Concepts — the same kernel works for any `IndexType` that
  satisfies the concept interface.

The LayerMap multi-type column structure is **absent** in soillib — it simplifies to
`height + sediment` float buffers, sacrificing geological stratigraphy for GPU parallelism.

---

## 6. Vegetation-Erosion Coupling

From SimpleHydrology `vegetation.h`:

```rust
struct Plant {
    pos: Vec2,
    size: f32,  // grows toward max_size
}

fn root_density_contribution(plant: &Plant, cells: &mut CellMap):
    // Write weighted root density to 3x3 neighborhood
    cells[plant.pos + (0,0)].root_density += factor * 1.0
    cells[plant.pos + (±1,0)].root_density += factor * 0.6
    cells[plant.pos + (±1,±1)].root_density += factor * 0.4
```

`root_density` directly scales `deposition_rate` in the water particle equation:
`eff_deposition = deposition_rate * (1 - root_density)`.

Plants spawn where `discharge < max_discharge` (not in rivers) and
`slope < max_steep`. They die if discharge rises (flood) or if random death check
triggers. This creates a feedback loop:
- Low-discharge flat land -> plants grow -> roots resist erosion -> stable soil
- High-discharge channels -> no plants -> easy erosion -> channel deepens

---

## 7. What We Can Port to Rust

### High-value, moderate complexity

| Feature | Rust approach | Benefit |
|---|---|---|
| Momentum + discharge maps | Two extra `f32` buffers per cell; exponential blend each erosion pass | Meandering rivers for free |
| Variable-timestep particle | `speed = normalize(speed) * cell_diagonal` prevents tunneling | Fixes current erosion artifacts |
| Finite-diff surface normal | 5-point stencil instead of mesh normal | Removes saddle/diagonal bias |
| Root-density erosion resist | `f32` per cell, updated by vegetation sim | Connects erosion to biomes |

### High-value, high complexity

| Feature | Rust approach | Benefit |
|---|---|---|
| Soil column (LayerMap) | `Vec<Vec<SoilLayer>>` per cell with type enum | True stratigraphy, exposed rock faces |
| Sediment type graph | `SoilType` enum + per-type `SoilParams` struct | Rock->gravel->sand->soil realism |
| Water seepage | Downward saturation diffusion pass after erosion | Groundwater, springs, waterlogging |

### Lower priority (infrastructure cost high)

- GPU parallelism (soillib approach) — not needed at our scale
- Full weather advection — our existing moisture model covers the need
- Memory pool for soil sections — only matters if we run millions of erosion cycles

### Recommended next step

Implement the momentum+discharge map enhancement to our existing hydraulic erosion.
It requires only two extra `f32` fields on the cell struct and an exponential blend
step after each erosion batch. This alone produces the meandering behavior and
eliminates the braided/random-walk appearance of current erosion channels.

---

## Implementation Status & Next Steps

| Feature | Status | Notes |
|---|---|---|
| Momentum + discharge maps | **NOT YET** | Highest-ROI port — two `f32` buffers + exponential blend give meandering rivers without structural changes |
| Root-density erosion resistance | **NOT YET** | Would connect vegetation stage to erosion; requires `root_density: f32` per cell updated by plant sim |
| Water seepage | **PARTIALLY DONE** | Darcy's law groundwater diffusion exists in `moisture.rs`, but not the full saturation/spring model (saturation → 1.0 forces water to surface) |
| Soil column LayerMap | **NOT YET** | High complexity; linked-list-per-cell stratigraphy is a large architectural change — future work |
| Wind / weather | **DONE** | Curl noise wind field implemented, similar to SoilMachine's Perlin noise approach (section 4 above); evolves every 10 ticks with terrain damping |
