# Analytical Erosion Research

Sources:
- Reddit r/proceduralgeneration (post 1nl4o0l) — "Cheap, gorgeous erosion, realtime generated each frame"
- Paper: "Physically-based Analytical Erosion for Fast Terrain Generation" — Tzathas, Gailleton, Steer, Cordonnier (2024, Computer Graphics Forum / EG)
  - PDF: https://hal.science/hal-04525371/file/Analytical_Terrains_EG.pdf

---

## The Technique

Both sources converge on the same core idea: replace particle simulation with
analytical solutions to the stream power law (SPL), the geomorphological equation
governing how rivers incise rock under tectonic uplift.

The stream power law states erosion rate = K * A^m * S^n, where A is upstream
drainage area and S is local slope. Traditional droplet erosion approximates this
via thousands of particle paths. The analytical approach solves the SPL directly as
a closed-form 1D equation along each river network branch, then lifts that to 2D
using a multigrid iterative process that alternates between:
1. Computing the river network (drainage area, flow direction tree)
2. Solving 1D analytical elevation profiles along each river channel

**Time is no longer a stepping criterion** — it becomes a slider parameter to the
mathematical function, controlling landscape "age" from subtle rounding to fully
developed mountain ranges with mature drainage.

The Reddit post demonstrated a variant running per-frame in real-time by computing
these analytical river solutions on GPU, producing sharp ridgelines and wide
valleys without any particle stepping.

---

## How It Differs from Our Droplet Erosion

| Property | Droplet (current) | Analytical SPL |
|---|---|---|
| Method | Simulation, stochastic | Closed-form math |
| Iterations needed | 50,000–200,000 droplets | 1 pass + multigrid convergence |
| Basin filling | **Yes — silt pools in depressions** | No — SPL is incision-only by formulation |
| Landslides/hillslope | Not modeled | Incorporated as separate diffusion term |
| Determinism | Noisy, seed-dependent | Deterministic given same inputs |
| "Time" control | Baked into droplet count | Explicit parameter |
| River width | Controlled by brush radius hack | Emerges from drainage area (physically correct) |

Our current droplet erosion fills ocean basins and enclosed lake beds with silt
because sediment capacity drops to zero when a droplet slows in flat water — the
analytical SPL is a pure incision model and does not accumulate sediment in
depressions by default.

---

## Performance

- The Tzathas 2024 paper targets large-scale terrain (512x512 to 2048x2048) and
  reports "fast" generation without specifying exact ms, but the multigrid approach
  is described as converging in far fewer iterations than time-stepping.
- The Reddit real-time variant runs each frame on GPU, implying sub-16ms for
  reasonable resolutions.
- For our 256x256 grid the analytical approach should be extremely fast — the
  multigrid tree traversal is O(N log N) in grid cells, and 256x256 = 65,536 cells
  is trivial compared to the paper's benchmark sizes.

---

## Applicability to terrain-gen-rust

Strong candidate to replace or supplement our droplet erosion stage:

1. **Fix basin silt problem**: SPL incises, does not deposit into enclosed basins.
   Ocean floors stay clean. Lake beds are not silted.
2. **Correct river scaling**: Drainage area drives channel width physically — wide
   valleys near mouths, narrow headwaters — without the erosion-brush-radius hack.
3. **Hillslope diffusion add-on**: The paper includes a separate Laplacian diffusion
   pass for soil creep on slopes, which we could run after SPL to smooth ridges.
4. **Time slider for worldgen stages**: Early-world terrain can use t=small (subtle
   carving), mature continents use t=large (deep valleys, wide rivers).
5. **Implementation path**: Build a drainage area map (we likely have flow
   accumulation already for hydrology), then solve SPL analytically per river
   segment. The multigrid acceleration can be added later for speed.

The main tradeoff is that pure SPL does not model sediment transport or alluvial
fans — depositional landforms (deltas, flood plains) need an additional deposition
layer on top. For a civ-sim scale game this is acceptable: erosion handles the
mountains and rivers, deposition can be approximated by smoothing floodplains
separately.

---

## Recommended Next Step

Prototype a CPU implementation of drainage-area + analytical SPL for our 256x256
heightmap, comparing output against our current droplet pass. Key metric: do ocean
basins stay flat while river valleys carve correctly?

---

## Known Limitations & Future Work

**Coastline gully artifact.** SPL creates anomalously deep gullies at coastlines
because drainage area and slope both peak at the ocean boundary — every inland
pixel's flow converges there while the elevation drops sharply to sea level. The
incision term `K * A^m * S^n` is maximized exactly where we least want erosion.

**Current workaround.** Skip tiles whose flow drains directly to an ocean cell.
This is physically defensible: real river mouths are depositional environments
(deltas, estuaries), not incisional ones. Sediment decelerates and drops out at
the coast rather than carving deeper.

**Proper fix: add a deposition pass after SPL.** The Tzathas 2024 paper itself
acknowledges that "depositional landforms need an additional deposition layer" on
top of the incision-only SPL solve. A sediment-capacity transport rule (deposit
when capacity drops below load) would naturally infill the coastline gullies with
alluvial deposits.

**SoilMachine's particle-descent approach** solves this organically: each water
particle deposits its remaining sediment at end-of-life (`height[ipos] +=
drop.sediment`), so coastlines accumulate deltas instead of incising. Porting the
particle descent deposition logic would complement the analytical SPL pass.

**Multi-pass iterative SPL** (recompute drainage area between passes) helps the
terrain relax toward equilibrium and softens artifacts, but does not fix the
fundamental issue — the coastline incision is a structural consequence of the SPL
equation, not a convergence problem. Deposition is the missing physics.
