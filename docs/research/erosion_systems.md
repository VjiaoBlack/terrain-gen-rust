# Erosion Systems Research

## What went wrong with our implementation

### Thermal erosion issues

Our implementation (`thermal_erosion` in `terrain_pipeline.rs`) uses a 4-neighbor scan that finds the steepest downslope neighbor and transfers `c * (diff - threshold)` of material per iteration. Problems:

- **Sequential scan order creates bias.** The nested `for y / for x` loop processes top-left before bottom-right. Each iteration updates cells that later iterations in the same pass then read. This causes directional drift — material flows southwest more readily than northeast, producing subtle diagonal ridges.
- **No convergence guarantee.** With `c = 0.5` and `iters = 40`, material can oscillate between two cells rather than settling. Real talus relaxation should converge to a stable angle; this can instead amplify irregularities.
- **Threshold too small for discrete grids.** `thermal_threshold = 0.0156` on a 0-1 normalized heightmap means nearly every cell erodes every iteration. Forty passes at that sensitivity effectively blurs everything flat. The result looked like rounded clay hills, not rocky scree.
- **When thermal erosion works:** A small number of passes (5-10) after initial noise generation can knock off sharp noise spikes and soften cliff faces. It works best as a light post-processing step, not a shaping tool.

### Droplet erosion issues

Our implementation (`droplet_erosion`) correctly follows the Lague/Beyer approach — bilinear interpolation, inertia blending, erosion brush radius, sediment capacity based on slope × speed × water. It was disabled because:

- **Interaction with `priority_flood`.** We run `priority_flood` before droplets to fill depressions. Droplets then re-erode those filled basins, but the filled floor is flat — so droplets stop immediately (near-zero gradient), dump all sediment in one spot, and create a raised bump in the depression center. That bump then acts as a new dam. Repeat → cliff-edged lake with raised center mound.
- **No drainage awareness.** Droplets do not know whether a cell is a lake bed or a hillside. They deposit wherever sediment > capacity regardless of context. Lakes should be deposition-free zones or handled separately.
- **`max_lifetime = 30` is short.** On a 256×256 map, many droplets run out of lifetime mid-slope and drop sediment at arbitrary elevation, not at the natural deposition zone (valley floor or delta).

### River carving issues

Our `carve_rivers` function sets `target = river_h - bed_depth` and only lowers cells, never raises them. Problems:

- **Carving against flat priority-flooded terrain.** After `priority_flood`, depressions are filled to a flat plane. Carving into that plane creates a sharp vertical wall at the edge of the flat region — exactly the "cliff-sided lake" artifact.
- **`bed_depth` driven by log(accumulation)** grows moderately but ignores the absolute elevation context. A river at altitude 0.3 gets carved the same depth as one at 0.05, which may already be near the water table.
- **No upstream-to-downstream ordering.** Carving processes all river cells in arbitrary order. Downstream cells get carved first; when the upstream cell is carved later, it may end up higher than the downstream cell, creating a backwards slope.

---

## Approaches

### 1. Improved hydraulic erosion (Lague/Beyer)

Sebastian Lague's Unity implementation (MIT license, [GitHub](https://github.com/SebLague/Hydraulic-Erosion)) implements the Beyer thesis algorithm. Key improvements over our current code:

- **Separate erosion and deposition passes.** Collect all deltas into a scratch buffer; apply once per iteration. This eliminates order-dependent artifacts.
- **Erosion brush with precomputed weights.** Weights are `max(0, radius - dist)` normalized. Our code does this correctly but the brush weights should be precomputed once, not recalculated per droplet step.
- **Cap per-cell erosion per step.** Prevent a single droplet from removing more than some fraction of a cell's height. Prevents the lake-bottom spike problem.
- **Run BEFORE priority_flood.** Erosion should carve natural drainage paths; let the hydrology stage discover what remains flooded afterward, not the reverse.
- **Droplet starting positions.** Bias toward high-elevation cells using rejection sampling or a weighted distribution. This concentrates erosion on hillsides where it belongs.

Key parameters that matter most for our 256×256 world-gen use case:

| Parameter | Recommended | Why |
|-----------|-------------|-----|
| `erosion_radius` | 4–5 | Produces 3-5 tile valley floors, avoids 1-cell trenches |
| `num_droplets` | 30k–60k | ~0.5–1× pixels; more shows diminishing returns |
| `max_lifetime` | 60 | Allows droplets to reach valley floors |
| `erode_speed` | 0.3 | Balanced with deposit_speed |
| `deposit_speed` | 0.3 | |
| `inertia` | 0.05 | Low = follows gradient tightly; higher = smoother paths |
| `capacity_factor` | 4 | Tune down if over-eroding |

### 2. Stream power erosion

The Stream Power Incision Model (SPIM) from geomorphology: `E = K * A^m * S^n`. Here A is upstream drainage area, S is slope, K is erodibility. This gives physically meaningful valley widening proportional to discharge — large rivers cut wide valleys, headwaters cut narrow gullies.

For game purposes a simplified version works: compute flow accumulation `A` first (we already do), then lower each cell proportional to `A^0.5 * slope`. Run this for 5-20 iterations. This produces the characteristic V-valley → U-valley progression. The `dandrino/terrain-erosion-3-ways` repo ([GitHub](https://github.com/dandrino/terrain-erosion-3-ways)) implements this as the "river networks" approach in Python.

Limitation: produces only incision, no sediment deposition. Must be paired with a deposition step.

### 3. Multi-scale erosion

Run erosion at multiple scales in sequence:

1. **Coarse pass (large brush, few droplets):** Establish major valley shapes.
2. **Medium pass (brush ~3):** Secondary drainage network.
3. **Fine pass (brush ~1-2, few iterations):** Surface texture and gullying.

This avoids the common failure where running erosion at one scale over-smooths large features while under-developing small ones.

### 4. GPU erosion

GPU compute shaders can run 1M droplets in ~10 seconds on an RTX 3060, enabling interactive parameter tuning. The main implementation challenge is race conditions: multiple threads eroding the same cell simultaneously. Solutions:

- **Tiled approach:** Divide map into tiles, process non-adjacent tiles in parallel, alternate tile patterns each pass.
- **Atomic operations:** Some GPU erosion papers use atomic float adds (available in Vulkan/wgpu).
- **Shallow water equations (SWE):** Grid-based approach without races; each cell updates from its 4 neighbors only. Produces lakes and rivers naturally but is more complex and slower per step. The `bshishov/UnityTerrainErosionGPU` repo implements SWE hydraulic + thermal erosion.

For our use case (offline world-gen, terminal game), GPU is not necessary. A well-tuned CPU implementation is fast enough at 256×256.

---

## River meandering

Real meanders form because erosion is greatest on the outer bank of a curve (centrifugal force pushes water outward), while deposition occurs on the inner bank (point bar). The curve amplifies over time until a cutoff occurs, leaving an oxbow lake.

The 2023 ACM SIGGRAPH paper "Authoring and Simulating Meandering Rivers" ([HAL](https://hal.science/hal-04227965)) provides a physically-based migration equation usable in offline terrain generation. Key insight: curvature-driven migration + cutoff detection gives geologically believable meander belts.

For a simpler implementation: after computing the river skeleton from flow accumulation, apply a spline with controlled curvature, then widen the valley proportional to sinuosity. Nick McDonald's procedural hydrology approach ([blog](https://nickmcd.me/2020/04/15/procedural-hydrology/)) adds a "stream map" and "pool map" that together let rivers preferentially deepen existing channels and fill depressions realistically.

---

## Sediment transport and deposition

Three deposition landforms matter for realism:

- **Alluvial fans:** Form where a confined steep channel enters a flat plain — velocity drops, sediment spreads in a cone. In game terms: detect "constriction → wide flat" transitions; deposit a radial fan of raised sediment.
- **Deltas:** Where river meets coast or lake. Deposit sediment in a triangular lobe, raising the shore. In practice: when river accumulation reaches a water body, deposit `sediment * (1 - distance/delta_radius)`.
- **Floodplains:** Long-term lateral deposition adjacent to large rivers. Model as a gradual raise of cells within 3-5 tiles of high-accumulation rivers, proportional to river discharge.

The key to preventing over-flat terrain: sediment deposition should be proportional to the *change* in transport capacity (slope × velocity), not absolute sediment load. Sediment drops out when the river slows, not uniformly everywhere.

---

## World-gen vs runtime erosion

| | World-gen (offline) | Runtime |
|---|---|---|
| Budget | Seconds to minutes | <16ms per frame |
| Approach | Full particle simulation, SPIM | Shallow water sim on small patch |
| Detail | Global drainage network | Local reactive erosion |
| For us | Run once at map generation | Not needed yet |

Our game does world-gen offline once per seed. We can afford 2-5 seconds for erosion on a 256×256 map. This is enough for 30k-60k droplets plus a SPIM pass.

---

## Performance estimates

On a single CPU core (Rust, 256×256 map):

| Operation | Estimate |
|-----------|----------|
| 10k droplets, lifetime 60 | ~50ms |
| 50k droplets, lifetime 60 | ~250ms |
| SPIM 10 iterations | ~10ms |
| Thermal erosion 10 iters | ~5ms |
| Full erosion pipeline | ~300-400ms total |

This is acceptable for world-gen. Rayon parallelism (already used in this codebase) can bring droplet erosion down ~4× on a quad-core.

---

## Recommended approach for our game

**Phase 1 — fix the ordering bug (high impact, low effort):**
Move `droplet_erosion` to run BEFORE `priority_flood`. This eliminates the lake-floor spike artifact entirely. Increase `max_lifetime` to 60.

**Phase 2 — replace river carving with SPIM:**
Instead of `carve_rivers` (which sets hard targets), use a flow-accumulation-proportional lowering: for each cell, `heights[i] -= k * accum[i].sqrt() * slope[i]`. Run 5-10 iterations. This produces natural V-shaped valleys that widen downstream. Then run `priority_flood` to find the resulting lakes.

**Phase 3 — light thermal erosion as post-pass:**
Run 5 iterations of thermal erosion with `threshold = 0.05` (higher than current) after droplet erosion to knock off residual sharp spikes without flattening valleys. Fix the sequential scan order: process in alternating directions each pass (forward/backward rows, forward/backward columns).

**Phase 4 — sediment deposition zones:**
After erosion is stable, identify high-accumulation cells at low slope (valley floors, coastal flats) and slightly raise them proportional to upstream sediment budget. This creates alluvial plains and deltas without a full particle budget.

---

## Implementation sketch (Rust)

```rust
pub fn run_erosion_pipeline(heights: &mut [f64], w: usize, h: usize, config: &PipelineConfig) {
    // 1. SPIM: flow-proportional incision (replaces carve_rivers)
    let accum = compute_flow_accumulation_from_heights(heights, w, h);
    let slope = compute_slope(heights, w, h);
    for i in 0..w * h {
        let incision = config.spim_k * accum[i].sqrt() * slope[i];
        heights[i] -= incision.min(config.spim_max_cut);
    }

    // 2. Droplet erosion (BEFORE priority_flood)
    droplet_erosion(heights, w, h, config); // max_lifetime=60, num_droplets=40k

    // 3. Light thermal erosion (alternating scan direction)
    thermal_erosion_bidirectional(heights, w, h, 0.05, 0.3, 5);

    // 4. Now fill depressions — lakes form at natural low points
    priority_flood(heights, w, h);

    // 5. Sediment deposition pass (alluvial plains)
    let accum2 = compute_flow_accumulation_from_heights(heights, w, h);
    for i in 0..w * h {
        if slope[i] < config.deposition_slope_threshold && accum2[i] > config.deposition_min_accum {
            heights[i] += config.deposition_rate * accum2[i].ln();
        }
    }
}
```

---

## References

- [Sebastian Lague — Hydraulic Erosion (GitHub)](https://github.com/SebLague/Hydraulic-Erosion)
- [Hans Theobald Beyer — Implementation of a method for hydraulic erosion (thesis)](https://www.firespark.de/?id=project&project=HydraulicErosion)
- [henrikglass/erodr — C implementation of Beyer's algorithm](https://github.com/henrikglass/erodr)
- [dandrino/terrain-erosion-3-ways — Simulation, GAN, river networks](https://github.com/dandrino/terrain-erosion-3-ways)
- [Nick McDonald — Procedural Hydrology (stream + pool maps)](https://nickmcd.me/2020/04/15/procedural-hydrology/)
- [Job Talle — Simulating hydraulic erosion (snowball model)](https://jobtalle.com/simulating_hydraulic_erosion.html)
- [Frozen Fractal — Around the World 23: Hydraulic erosion](https://frozenfractal.com/blog/2025/6/6/around-the-world-23-hydraulic-erosion/)
- [Ivo van der Veen — Improved terrain generation using hydraulic erosion (Medium)](https://medium.com/@ivo.thom.vanderveen/improved-terrain-generation-using-hydraulic-erosion-2adda8e3d99b)
- [Balazs Jako — Fast Hydraulic and Thermal Erosion on the GPU (CESCG 2011)](https://old.cescg.org/CESCG-2011/papers/TUBudapest-Jako-Balazs.pdf)
- [Authoring and Simulating Meandering Rivers (ACM TOG 2023)](https://hal.science/hal-04227965)
- [rj00a/heightmap-erosion — Rust parallel erosion (Beyer/Andrino)](https://github.com/rj00a/heightmap-erosion)
- [Stream Power Incision Model — Lague 2014 (ESPL)](https://onlinelibrary.wiley.com/doi/10.1002/esp.3462)
- [Terrain Erosion on the GPU — aparis69](https://aparis69.github.io/public_html/posts/terrain_erosion.html)
