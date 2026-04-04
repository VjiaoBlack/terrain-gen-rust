# Water Simulation Research

## Current system (heightfield flow)

Tiles are assigned `Terrain::Water` at generation time by a `water_level` threshold; rivers are carved by the D8/flow-accumulation pipeline (`03_hydrology.md`). Spring floods are a post-pass marking low-elevation tiles near rivers as `FloodWater`. Water has no runtime dynamics.

### What works
- Fast: zero per-frame CPU cost, river positions are stable
- River networks are topologically correct (flow accumulation, valley carving)
- Spring flood season adds believable visual variation
- Integrates cleanly with pathfinding (water = impassable)

### What doesn't
- Water is purely cosmetic after gen; a dam, flood, or drought has no effect
- No depth — a swamp puddle and a deep river look identical to the sim
- Lakes are implicit (any depression below `water_level`) — no realistic pooling
- No sediment transport at runtime; rivers don't shift over gameplay time
- Waterfalls, backflow, pressure equalization are absent

---

## Approaches

### 1. Improved heightfield (pipe model)

The **virtual pipe model** (O'Brien 1995, Stam 2008 GDC) extends the static heightfield into a dynamic one. Each cell stores a water depth `w[x,y]`. Four virtual pipes connect each cell to its cardinal neighbors; flux through each pipe is driven by the hydrostatic height difference `(terrain[x,y] + w[x,y]) - (terrain[nx,ny] + w[nx,ny])`. This is integrated per timestep with a damping term.

**Complexity:** O(N) per frame, fully vectorizable. 65k cells at 60fps with four neighbor passes is ~15M float ops/frame — trivial on modern CPUs.

**What you get:** water flows downhill, pools in depressions, fills lakes, floods valleys. This is what the hydraulic-erosion GPU papers use at 2048² in <2ms.

**Limits:** Cannot represent underground pressure propagation or 3D turbulence. Acceptable for top-down colony sim.

### 2. Shallow water equations (SWE)

The SWE are a depth-averaged form of Navier-Stokes. They add a **velocity field** `(u,v)` alongside depth `h`. The update equations:

```
∂h/∂t  = -∇·(h·v)
∂(hv)/∂t = -∇·(hv⊗v) - g·h·∇(h+b) + friction
```

This produces vortices, momentum carry, inertia — water "sloshes" past obstacles and forms realistic eddies. It is more accurate than the pipe model for fast-moving rivers.

**Complexity:** O(N) per frame, ~3–5× heavier than the pipe model. CFL timestep: `dt ≤ dx/sqrt(g·h_max)` — at 256² with 5m max depth, `dt_max ≈ 0.14s`, fine for 60fps.

**What you get:** momentum-driven flow, backwater effects, flood waves.

**Limits:** Still heightfield; no underground pressure. Harder to stabilize than the pipe model.

### 3. Particle-based (SPH / FLIP)

**SPH:** each particle carries mass/velocity/density; forces from kernel-weighted neighbor sums. 3000 particles achieves ~300fps on mid-range GPU; 65k particles (one per tile) would require GPU acceleration.

**FLIP:** hybrid — particles carry velocity, a background grid handles pressure projection. Low numerical dissipation; used in Shadow of the Tomb Raider and VFX pipelines.

**Rust crate:** `salva2d` (dimforge) implements SPH/DFSPH with nalgebra, couples to rapier.

**Verdict:** overkill for a top-down colony sim. Useful only for decorative splash FX, not bulk water simulation.

### 4. Hybrid approaches

Best-practice for colony sims: **pipe model for bulk water dynamics + particle pass for visual detail**. Specifically:

1. Pipe model runs every game tick (not every render frame) at reduced resolution or with subcycling
2. Sediment transport added to the pipe model (see below)
3. Optional: SPH particle burst for waterfall/splash rendering only (not simulated mass)

---

## How other games do it

**Dwarf Fortress:** 7 discrete depth levels per tile (1=puddle, 7=full). Cellular automaton: water moves to lower neighbors each tick. Pressure is a "lazy model" — a column of 7/7 tiles pushes water uphill until equalized. Cheap, but diagonal gaps break pressure and there is no momentum.

**RimWorld (vanilla):** water is terrain-only, no runtime simulation. Mods (Dubs Bad Hygiene) add pipe networks but still no fluid dynamics.

The DF discrete-depth approach is the best fit for player-legible gameplay without full physics overhead.

---

## Sediment transport

The **Hjulström curve** defines three regimes by flow velocity and grain size:
- **Erosion:** velocity above the upper curve lifts sediment from the bed
- **Transport:** sediment stays suspended between curves
- **Deposition:** velocity falls below the lower curve, sediment settles

For a game approximation: `capacity = K_c * depth * velocity²`. If `sediment < capacity`, erode the bed; if `sediment > capacity`, deposit. This is the same formula used in the droplet erosion model (`07_hydraulic_erosion.md`) and extends naturally to the pipe model by computing a local velocity magnitude from inter-cell flux.

Key insight: fine silt deposits at lower velocities than gravel — natural sorting (coarse near mountains, fine in deltas). Approximation: two sediment buckets (coarse/fine) with different deposition thresholds.

---

## Performance estimates for 256×256

| Method | Ops/frame @60fps | Single-core ms/frame | Feasible? |
|---|---|---|---|
| Static heightfield (current) | 0 | 0 | Yes |
| Pipe model (4 neighbors) | ~1M | ~0.3ms | Yes |
| SWE (full velocity field) | ~3M | ~1ms | Yes |
| SPH (65k particles, naive) | ~650M | >16ms | No (GPU only) |
| SPH (3k particles, decorative) | ~30M | ~2ms | Marginal |
| DF-style cellular automaton | ~262k | ~0.1ms | Yes |

The pipe model and SWE are both comfortably within budget. The pipe model is the better starting point — simpler to implement and debug, and sufficient for pooling lakes and flowing rivers.

---

## Recommended approach for our game

**Phase 1 — DF-style discrete depth (low effort, high gameplay value)**
Add a `water_depth: u8` field (0–7) to tiles. Each game tick, propagate depth using a simple cellular automaton (water flows to lower total-height neighbors). This adds pooling, flooding, and draining with no floating-point complexity. Matches the aesthetic of the DF/RimWorld tier we target.

**Phase 2 — Pipe model for realistic dynamics**
Replace the CA with a pipe-model solver. Store `flux[4]` per tile (N/S/E/W). Update depth from net flux. Add sediment transport using the Hjulström approximation. This enables rivers to shift their course over long game time, deltas to form, and floods to spread realistically.

**Phase 3 (optional) — SWE for momentum**
If river dynamics feel sluggish, promote to full SWE. Only needed if inertia/eddies matter for gameplay.

---

## Implementation sketch (Rust)

```rust
struct WaterCell {
    depth: f32,      // meters above terrain
    flux: [f32; 4],  // outflow N/S/E/W
    sediment: f32,
}

fn update_water(cells: &mut [WaterCell], terrain: &[f32], dt: f32) {
    // Pass 1: update flux from hydrostatic head
    for each cell i with neighbors j[d]:
        let dh = (terrain[i] + cells[i].depth) - (terrain[j] + cells[j].depth);
        cells[i].flux[d] = (cells[i].flux[d] + dt * 9.81 * dh) * 0.99;
        cells[i].flux[d] = cells[i].flux[d].max(0.0);
    // Scale flux: can't drain more than available water
    let scale = (cells[i].depth / dt / total_outflux).min(1.0);
    cells[i].flux.iter_mut().for_each(|f| *f *= scale);

    // Pass 2: depth from net flux
    for each cell i:
        cells[i].depth += (inflow[i] - outflow[i]) * dt;
        cells[i].depth = cells[i].depth.max(0.0);
}
```

Run at 10Hz (every 6 render frames) — water dynamics are slow relative to 60fps rendering. Use `rayon::par_chunks_mut` on Pass 1 for easy parallelism.

---

## References

- [Fast Water Simulation for Games Using Height Fields (GDC 2008)](https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc08/slides/S6509i1.pdf)
- [Real-time Simulation of Large Bodies of Water — Müller et al.](https://matthias-research.github.io/pages/publications/hfFluid.pdf)
- [Fast Hydraulic Erosion Simulation on GPU — Št'ava et al.](https://inria.hal.science/inria-00402079/document)
- [Water Simulation Methods for Games — Kellomäki 2012](https://www.modeemi.fi/~daemou/mindtrek12.pdf)
- [Hjulström Curve — Wikipedia](https://en.wikipedia.org/wiki/Hjulstr%C3%B6m_curve)
- [DF2014:Water — Dwarf Fortress Wiki](https://dwarffortresswiki.org/index.php/DF2014:Water)
- [DF2014:Pressure — Dwarf Fortress Wiki](https://dwarffortresswiki.org/index.php/DF2014:Pressure)
- [salva2d — Rust fluid simulation crate](https://github.com/dimforge/salva)
- [FLIP: A Low-Dissipation Particle-in-Cell Method — Brackbill & Ruppel](https://www.researchgate.net/publication/222452290_FLIP_A_Low-Dissipation_Particle-in-Cell_Method_for_Fluid_Flow)
- [Particle-Based Fluid Simulation for Interactive Applications — Müller et al. (SCA 2003)](https://matthias-research.github.io/pages/publications/sca03.pdf)
- [Unity Terrain Erosion GPU (SWE reference impl)](https://github.com/bshishov/UnityTerrainErosionGPU)
