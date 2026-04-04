# Wind System Research

## Approaches (simplest to most complex)

### 1. Static wind field

A fixed global wind direction vector. Per-tile variation comes from terrain occlusion: cells behind mountains get a reduced vector computed at generation time. No runtime cost beyond reading a stored array. Suitable for batch terrain generation but too rigid for a living simulation — wind never shifts, no gusts, no fronts.

### 2. Perlin turbulence wind

A global prevailing direction combined with per-tile noise from a Simplex field that advances over time. The noise offsets direction and magnitude locally, giving gusts and lulls without solving any equations. Cost is trivial: one noise lookup per tile per tick. A curl-noise variant (Bridson 2007) makes the 2D velocity field divergence-free without a pressure solve, giving physically plausible advection for fire and moisture. This is the practical sweet spot for most games.

### 3. Jos Stam stable fluids

Stam's 1999 paper (and the 2003 games-oriented simplification) implement incompressible Navier-Stokes on a regular grid in four steps: add forces, advect (semi-Lagrangian), diffuse (implicit), project (pressure solve). The pressure projection is a Poisson solve — typically 10–20 Jacobi iterations. On 256x256 that is ~13M multiply-adds per solve. Complexity is O(N), unconditionally stable, and ~300 lines of Rust. **This is the most capable option still viable at 60fps with SIMD.**

### 4. Full Navier-Stokes

Adds compressibility, temperature-driven convection, turbulence models (LES/RANS). ONI uses a variant for its gas grids. At 256x256 this runs ~5–15fps on CPU without GPU help — not viable alongside other sim systems.

---

## How other games do it

| Game | Wind model | Notes |
|---|---|---|
| **RimWorld** | Single scalar per weather event (0%–150%), no spatial field | Wind turbines vary output; no tile-level wind direction |
| **Dwarf Fortress** | Per-biome scalar derived from world latitude; fixed at embark | Drives windmills; no dynamic spatial field; separate world-gen weather model tracks fronts |
| **Oxygen Not Included** | Full per-cell gas/liquid pressure simulation (simplified N-S) | Runs at reduced tick rate; tiles are small (~2m), map is ~128x128 |
| **Factorio** | No wind in vanilla; pollution diffuses isotropically | Wind turbines are mod-only |
| **Minecraft** | No wind simulation | Particles are cosmetic |

Key lesson: every shipping colony sim with per-cell fluid wind runs it at 1–4 Hz, decoupled from the visual frame rate. None run a full N-S solver at 60Hz on the gameplay grid.

---

## Coupling with moisture and fire

### Moisture transport (rain shadow)

Wind direction determines which face of a terrain ridge is windward. Moisture is advected by the wind vector: each tick, a moisture scalar at cell (x,y) moves toward the downwind neighbor proportional to wind speed. When the moisture parcel crosses an elevation gain above a threshold (orographic lift), it precipitates at a rate proportional to the elevation delta. The leeward side receives dry air, creating a dynamic rain shadow. This works with any of the approaches above — even a static wind field — since the moisture advection itself is O(N) per tick.

### Fire spread

Cellular automata fire fits naturally with a wind field. Each burning cell spreads to neighbors with probability:

```
p_spread(neighbor) = base_rate * fuel(neighbor) * moisture_penalty(neighbor)
                     * wind_boost(dot(wind_vec, direction_to_neighbor))
```

The `wind_boost` term scales the ignition probability by the dot product of the wind vector with the direction toward each neighbor (positive = downwind, spreads faster). Long-range embers can skip to non-adjacent cells when wind speed exceeds a threshold (Alexandridis et al. 2011). This requires no extra fluid data — just the per-cell wind vector sampled at fire tick rate.

---

## Performance estimates for 256×256

| Approach | Memory | Time/frame (est.) | Notes |
|---|---|---|---|
| Static field | 0.5 MB (2×f32 per cell) | ~0 | Read-only |
| Perlin turbulence | 0.5 MB + noise state | < 0.5 ms | Trivially parallelizable |
| Stam stable fluids | 4–6 MB (4–6 velocity/pressure arrays) | 2–5 ms | With SIMD; 10–20 Jacobi iters |
| Full N-S | ~12 MB | 30–80 ms | Not viable at 60fps |

At 60fps the budget per frame is ~16 ms. Stam fluids at ~3 ms is roughly 20% of frame budget. Since fire and moisture can tick at 4–10 Hz (not every frame), the wind solve itself can also run at 10–20 Hz with the result cached per frame, cutting actual cost to < 0.5 ms/frame.

---

## Recommended approach for our game

**Use curl-noise wind as the default runtime system, with an optional Stam solver flag for experimentation.**

Rationale:
- Curl-noise gives a divergence-free vector field with realistic gusts for ~0 runtime cost. It integrates immediately with the existing Perlin-based moisture and biome pipeline.
- A static orographic wind shadow computed at terrain generation time (from prevailing direction vs. heightmap gradient) handles rain shadow without any per-tick fluid solve.
- Fire spread via wind-weighted CA probabilities needs only the per-tile wind vector, no pressure solve.
- The Stam solver is the right upgrade path if dynamic weather fronts (cyclones, shifting winds) become a design goal — it slots in as a drop-in replacement for the noise field.

---

## Implementation sketch (Rust)

```rust
// Stored per tile alongside moisture, elevation, etc.
pub struct WindField {
    /// u (east) and v (north) velocity components, m/s
    pub u: Vec<f32>,
    pub v: Vec<f32>,
    width: usize,
    height: usize,
}

impl WindField {
    /// Curl-noise wind: base prevailing direction + divergence-free noise.
    /// noise_fn should be a 3D Simplex sampler: f(x, y, t) -> f32
    pub fn update_curl_noise(&mut self, t: f32, prevailing: (f32, f32), noise_fn: &impl Fn(f32, f32, f32) -> f32) {
        let (pu, pv) = prevailing;
        let eps = 1.0;
        for y in 0..self.height {
            for x in 0..self.width {
                let fx = x as f32;
                let fy = y as f32;
                // Curl of scalar potential phi gives divergence-free field
                let phi_north = noise_fn(fx, fy + eps, t);
                let phi_south = noise_fn(fx, fy - eps, t);
                let phi_east  = noise_fn(fx + eps, fy, t);
                let phi_west  = noise_fn(fx - eps, fy, t);
                let idx = y * self.width + x;
                self.u[idx] = pu + (phi_north - phi_south) / (2.0 * eps);
                self.v[idx] = pv - (phi_east  - phi_west)  / (2.0 * eps);
            }
        }
    }

    /// Wind boost factor for fire spread toward a neighbor.
    pub fn spread_boost(&self, x: usize, y: usize, dx: i32, dy: i32) -> f32 {
        let idx = y * self.width + x;
        let dot = self.u[idx] * dx as f32 + self.v[idx] * dy as f32;
        1.0 + 0.3 * dot.max(0.0) // downwind bonus, never a penalty > -30%
    }
}
```

For the Stam upgrade path, the `woeishi/StableFluids` SIMD implementation is directly portable to Rust via `std::simd` or `packed_simd`. The pressure projection loop is the only non-trivial piece (~80 lines).

---

## References

- [Stam, J. "Stable Fluids" (SIGGRAPH 1999) — Clemson mirror](https://people.computing.clemson.edu/~dhouse/courses/817/papers/stam99.pdf)
- [Stam, J. "Real-Time Fluid Dynamics for Games" (2003) — CMU mirror](http://graphics.cs.cmu.edu/nsp/course/15-464/Fall09/papers/StamFluidforGames.pdf)
- [Bridson, R. "Curl-Noise for Procedural Fluid Flow" (SIGGRAPH 2007)](https://www.cs.ubc.ca/~rbridson/docs/bridson-siggraph2007-curlnoise.pdf)
- [woeishi/StableFluids — SIMD-optimized Stam implementation](https://github.com/woeishi/StableFluids)
- [dimforge/salva — particle fluid sim crate for Rust](https://github.com/dimforge/salva)
- [wickedchicken/stroemung — CFD in Rust with ndarray](https://github.com/wickedchicken/stroemung)
- [Alexandridis et al. "A new algorithm for simulating wildfire spread through CA" (ACM TOMACS 2011)](https://dl.acm.org/doi/10.1145/2043635.2043641)
- [Nick McDonald, "Procedural Weather Patterns" blog post](https://nickmcd.me/2018/07/10/procedural-weather-patterns/)
- [RimWorld Wiki — Wind turbine / Environment](https://rimworldwiki.com/wiki/Wind_turbine)
- [Dwarf Fortress Wiki — Weather](https://dwarffortresswiki.org/index.php/DF2014:Weather)

---

## Curl Noise Implementation (Current)

Replaced the Stam stable fluids solver (which produced a static field within each
season) with a curl noise vector field that evolves every 10 ticks.

**Architecture:**
- Two decorrelated Perlin noise fields generate `vx` and `vy` independently, 3
  octaves each, sampled with a time coordinate that advances each update.
- 15% prevailing wind bias is added to the curl output, giving a dominant
  direction while allowing local variation and reversal.
- Terrain damping: wind speed is reduced behind high terrain (orographic
  sheltering). Terrain deflection: wind vectors bend around steep gradients
  rather than passing through ridges.
- Switchable at config level via `WindModel::CurlNoise` (default) vs.
  `WindModel::Stam` in `SimConfig`.

**Research finding:** The terrain generation community (SoilMachine, World Machine,
Gaea) consistently uses noise-based wind fields rather than fluid solvers for
climate-scale wind simulation. Fluid solvers are reserved for small-scale effects
(fire plumes, gas diffusion in ONI). At world-gen scale, noise fields produce
equivalent climate patterns (rain shadows, prevailing belts) at a fraction of the
cost, and their stochastic nature better represents the chaotic mixing of real
atmospheric circulation than a laminar Stokes solution on a coarse grid.
