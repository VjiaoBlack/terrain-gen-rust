# Advanced Terrain Simulation Research

Compiled 2026-04-04. Focuses on cutting-edge techniques beyond the standard references
(Nick McDonald, Red Blob Games, Sebastian Lague).

---

## Table of Contents

1. [Terrain Generation Papers 2022-2025](#1-terrain-generation-papers-2022-2025)
2. [Novel Erosion Techniques](#2-novel-erosion-techniques)
3. [Vegetation Simulation](#3-vegetation-simulation)
4. [Geological Simulation](#4-geological-simulation)
5. [Atmosphere and Ocean](#5-atmosphere-and-ocean)
6. [Procedural Cities and Roads](#6-procedural-cities-and-roads)
7. [Hidden Gem Repos](#7-hidden-gem-repos)
8. [Applicability to Our System](#8-applicability-to-our-system)

---

## 1. Terrain Generation Papers 2022-2025

### 1.1 Terrain Diffusion (Goslin, 2025)

**Source:** [arxiv.org/abs/2512.08309](https://arxiv.org/abs/2512.08309)
**Code:** [github.com/xandergos/terrain-diffusion](https://github.com/xandergos/terrain-diffusion)

A diffusion-based framework that replaces Perlin noise for infinite terrain generation. The core
innovation is **InfiniteDiffusion** -- an algorithm that reformulates diffusion sampling for
unbounded domains with constant-time random access and seed consistency.

Architecture:
- Coarse planetary model generates basic world structure
- Core latent diffusion model produces realistic 46km tiles in latent space
- Consistency decoder expands latents into high-fidelity elevation maps
- Trained on ETOPO (30 arc-second) and WorldClim bioclimatic data

Performance: Streams entire worlds in real-time, demonstrated via full Minecraft integration.
Outpaces orbital velocity by 9x on a consumer GPU.

**Quality:** High. Published research with working code and Minecraft mod.
**Our scale:** Not directly applicable (requires GPU + PyTorch inference), but the *idea* of
hierarchical coarse-to-fine generation with learned priors is adaptable. We could use a
pre-generated diffusion output as initialization for our 256x256 grid.

### 1.2 Real-Time Terrain Enhancement with Controlled Procedural Patterns (Grenier et al., 2024)

**Source:** [Computer Graphics Forum 43(1)](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14992)
**PDF:** [hal.science/hal-04360714](https://udl.hal.science/hal-04360714v2)

Structured noise that enhances terrains in real-time by adding spatially varying erosion-like
patterns. Built on **Phasor noise** adapted to terrain characteristics:
- Patterns cascade (narrow nested into large)
- Controlled by water flow direction and slope orientation
- Resolution-independent with large amplification factors
- Parallel GPU implementation achieves real-time

**Quality:** Top-tier (Eurographics 2024). Elegant approach.
**Our scale:** The concept of flow-oriented detail enhancement is perfect for us. We could
adapt a simplified version: use our existing flow maps to orient procedural detail at render
time. Even in terminal, directional ASCII patterns could convey erosion direction.

### 1.3 Terrain Descriptors for Landscape Synthesis, Analysis and Simulation (Argudo et al., 2025)

**Source:** [Computer Graphics Forum](https://onlinelibrary.wiley.com/doi/10.1111/cgf.70080)

A comprehensive survey/framework for terrain descriptors -- the mathematical signatures that
characterize terrain morphology. Useful for both analysis (classifying terrain types) and
synthesis (generating terrain that matches a target descriptor).

**Quality:** Survey-level paper from a top venue.
**Our scale:** Descriptors could be used to validate our generated terrain against real-world
morphological targets. Lightweight to compute.

### 1.4 Interactive Authoring of Terrain using Diffusion Models (Lochner et al., 2023)

**Source:** [Computer Graphics Forum](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14941)
**PDF:** [hal.science/hal-04324336](https://hal.science/hal-04324336/file/paper1145_CRC_HAL.pdf)

Artists sketch ridges and drainage networks, select styles from a terrain database, then an
ensemble of diffusion models synthesizes matching terrain. Key innovation: algorithmic
extraction of "terrain signatures" (ridge networks, cliff lines, flat regions) from real
heightmaps, used to condition generation.

Supports inverse terrain modeling -- generate a structural replica from an extracted signature,
then edit interactively.

**Quality:** High (Pacific Graphics 2023).
**Our scale:** The signature extraction concept is useful. We could extract signatures from our
generated terrain to classify and validate it.

### 1.5 TerraFusion (2025)

**Source:** [arxiv.org/abs/2505.04050](https://arxiv.org/abs/2505.04050)

Joint generation of terrain geometry AND texture using a single Latent Diffusion Model,
avoiding the error accumulation of two-stage approaches.

**Quality:** Preprint, interesting but less relevant to our terminal-rendered system.

### 1.6 Procedural Terrain with Style Transfer (2024)

**Source:** [arxiv.org/html/2403.08782](https://arxiv.org/html/2403.08782v1)

Combines procedural generation with neural style transfer, drawing morphological style from
real-world heightmaps. Produces terrains that are diverse yet geologically plausible.

**Quality:** Solid paper.
**Our scale:** Pre-compute styled heightmaps offline, use as initialization.

---

## 2. Novel Erosion Techniques

### 2.1 Large-Scale Terrain Authoring through Interactive Erosion Simulation (Schott et al., 2023)

**Source:** [ACM Transactions on Graphics 2023](https://dl.acm.org/doi/10.1145/3592787)
**Code:** [github.com/H-Schott/StreamPowerErosion](https://github.com/H-Schott/StreamPowerErosion)

Uses the **Stream Power Equation** from geomorphology. The key insight: model terrain in the
"uplift domain" (how much the ground wants to rise) and compute emerging reliefs by simulating
the equilibrium between uplift and erosion. A fast approximation of drainage area and flow
routing enables interactive computation.

Tools: copy-paste mountain ranges, warp for folds/faults, point/curve elevation constraints.

**Quality:** Top-tier (SIGGRAPH 2023). 48 stars, MIT license, C/C++/GLSL.
**Our scale:** HIGHLY RELEVANT. The stream power equation is simple:
`dz/dt = U - K * A^m * S^n` where U=uplift, A=drainage area, S=slope, K=erodibility.
This is cheaper than particle-based erosion and produces more geologically correct results at
large scales. We could implement this on our 256x256 grid.

### 2.2 Terrain Amplification using Multi-Scale Erosion (Schott et al., 2024)

**Source:** [ACM Transactions on Graphics 2024](https://dl.acm.org/doi/10.1145/3658200)
**PDF:** [hal.science/hal-04565030](https://hal.science/hal-04565030/file/2024-MultiScaleHydro-Author.pdf)

Amplifies a low-resolution terrain into high-resolution, hydrologically consistent terrain
using multi-scale erosion. Combines thermal, stream power erosion, and deposition at different
scales. Bridges physics-based erosion with procedural multi-scale modeling.

**Quality:** Top-tier (SIGGRAPH 2024).
**Our scale:** Perfect philosophy for us. Run coarse erosion on our 256x256 grid, then amplify
with procedural detail at render time.

### 2.3 Efficient Debris-Flow Simulation for Steep Terrain Erosion (Jain et al., 2024)

**Source:** [ACM Transactions on Graphics 2024](https://dl.acm.org/doi/10.1145/3658213)
**PDF:** [hal.science/hal-04574826](https://hal.science/hal-04574826/file/2024_Siggraph_Debris_Flow_Author_Version.pdf)

Addresses a real gap: standard hydraulic erosion fails on steep slopes near ridges. In these
low-drainage areas, **debris flow** (mud + rock mixtures) dominates. New mathematical
formulation derived from geomorphology, unified GPU algorithm for both fluvial and debris flow.

**Quality:** Top-tier (SIGGRAPH 2024).
**Our scale:** The insight matters more than the GPU implementation. On steep slopes, we should
switch from stream-power to a slope-dependent debris-flow model. Simple to implement:
debris erosion rate proportional to slope * contributing area.

### 2.4 FastFlow: GPU Flow and Depression Routing (Jain et al., 2024)

**Source:** [Computer Graphics Forum 2024](https://onlinelibrary.wiley.com/doi/10.1111/cgf.15243)

Novel GPU flow routing in O(log n) iterations, depression routing in O(log^2 n). Combined with
implicit time-stepping: **10 iterations and 0.1s captures 700,000 years of geomorphological
evolution** (vs 2.6s CPU).

**Quality:** Top-tier.
**Our scale:** The algorithmic insights apply even without GPU. The depression routing algorithm
(handling lakes/sinks in flow networks) is critical for realistic hydrology. At 256x256
(65K cells), even naive CPU approaches are fast enough, but the mathematical formulation
is cleaner than flood-fill approaches.

### 2.5 Forming Terrains by Glacial Erosion (Cordonnier et al., 2023)

**Source:** [ACM Transactions on Graphics (SIGGRAPH 2023)](https://dl.acm.org/doi/10.1145/3592422)
**PDF:** [inria.hal.science/hal-04090644](https://inria.hal.science/hal-04090644/file/Sigg23_Glacial_Erosion__author.pdf)

First solution for simulating glacial formation, evolution, and erosion over glacial/inter-glacial
cycles. Uses a **deep learning-based estimation of high-order ice flow** (the Shallow Ice
Approximation enhanced with learned corrections) and a multi-scale advection scheme for the
distinct timescales of glacier equilibrium vs. terrain erosion.

Produces: U-shaped valleys, hanging valleys, fjords, glacial lakes, cirques, aretes.

**Quality:** Top-tier (SIGGRAPH 2023). Groundbreaking for glacial terrain.
**Our scale:** The full ML ice flow is overkill, but the simplified Shallow Ice Approximation
(SIA) is tractable: ice flux proportional to thickness^5 * slope^3. We could approximate
glacial carving with a diffusion equation weighted by accumulated "cold" from our climate sim.

### 2.6 Unerosion: Simulating Terrain Evolution Back in Time (Yang et al., 2024)

**Source:** [Computer Graphics Forum (SCA 2024)](https://dl.acm.org/doi/10.1111/cgf.15182)
**PDF:** [cs.purdue.edu](https://cs.purdue.edu/cgvlab/www/resources/papers/Yang-CGF-2024-Unerosion.pdf)

Reverses erosion equations to recover plausible past topographies. Reformulates fluvial erosion,
sedimentation, and thermal erosion for backward simulation. Validated against geological findings
(historical riverbed heights).

**Quality:** High (SCA 2024). Novel concept.
**Our scale:** Fascinating for gameplay -- show the player how terrain looked in the past,
or generate "ancient" terrain states for lore. Mathematically elegant.

### 2.7 The "Fastest Erosion Algorithm Ever" (Procedural Pixels, 2024)

**Source:** [proceduralpixels.com](https://www.proceduralpixels.com/blog/terrain-hack-fastest-erosion-algorithm-ever)

A mathematical approximation, NOT a physical simulation. Layers ~100-200 "slab functions" at
different height thresholds, finds distance to height intersections within a search window,
converts to linear slope functions. ~30 lines of core code.

Performance: 100-300ms for 1024x1024 on RTX 3060. O(n^2) in kernel size.

**Quality:** Hack, not science. But visually convincing and incredibly fast.
**Our scale:** HIGHLY APPLICABLE. Could run on CPU for 256x256 in milliseconds. Good for
quick visual enhancement of terrain without proper erosion simulation. Use as a post-process.

### 2.8 Lattice Boltzmann for Shallow Water Erosion

**Source:** [Springer (book)](https://link.springer.com/book/10.1007/978-3-662-08276-8)
**Coupled LBM-MPM:** [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0045782521003716)
**Morphodynamic LBM:** [Springer](https://link.springer.com/article/10.1007/s00366-023-01842-7)

Lattice Boltzmann Methods solve the shallow water equations on a regular grid via collision-and-
stream operations on particle distribution functions. Natural fit for:
- Arbitrary terrain with variable depth
- Dry-wet boundary tracking
- Coupled sediment transport (bed load + suspended load)

A hybrid 3D model couples LBM (complex flow) with MPM (large soil deformations) via a sharp
interface coupling scheme.

**Quality:** Academic, well-established.
**Our scale:** LBM on a 256x256 grid is very feasible on CPU. The D2Q9 lattice (9 velocities
per cell) needs ~9 floats per cell = ~600KB. Each timestep is local (no global solves), making
it cache-friendly and parallelizable. This could replace our current flow simulation with
something more physically accurate while remaining fast.

### 2.9 Material Point Method (MPM) for Soil Erosion

**Source:** [ScienceDirect 2026](https://www.sciencedirect.com/science/article/pii/S0020740326003255)

Two-phase two-point MPM uses dual sets of Lagrangian material points on a shared Eulerian grid
to resolve soil-fluid interactions. Validated against dam break, wall-jet erosion, overtopping
erosion, tsunami overflow.

**Quality:** Academic, cutting-edge.
**Our scale:** MPM is computationally heavy (particles + grid transfers). Not practical for
real-time on our scale, but the physics insights (how soil behaves at saturation boundaries)
inform our simplified models.

### 2.10 SPH for Hydraulic Erosion

**Source:** [ResearchGate](https://www.researchgate.net/publication/227520146_Hydraulic_Erosion_Using_Smoothed_Particle_Hydrodynamics)

Couples SPH fluid simulation with erosion model for 3D terrain modification. Interactive at
up to 25,000 particles.

**Quality:** Established technique.
**Our scale:** We don't need full SPH. But the erosion coupling formulas (how particle velocity
and terrain slope determine sediment pickup/deposition rates) are useful reference for our
particle-based erosion.

---

## 3. Vegetation Simulation

### 3.1 Authoring Landscapes by Combining Ecosystem and Terrain Erosion (Cordonnier et al., 2017)

**Source:** [ACM Transactions on Graphics (SIGGRAPH 2017)](https://dl.acm.org/doi/10.1145/3072959.3073667)
**PDF:** [hal.science/hal-01518967](https://hal.science/hal-01518967/file/authoring-landscapes-combining.pdf)

The foundational paper on bi-directional feedback between erosion and vegetation. Key insight:
vegetation prevents erosion (root systems stabilize soil), while erosion shapes where vegetation
can grow. The framework simulates layered materials (rock, sand, humus) + vegetation layers
(grass, shrubs, trees) with mutual interactions.

Competition model: trees eliminated if they don't achieve minimum growth radius. Vegetation
cover maximized via projected canopy area optimization.

**Quality:** Top-tier (SIGGRAPH). Seminal work.
**Our scale:** DIRECTLY APPLICABLE. We already have biomes and terrain. Adding a simple
competition model:
- Each cell has a "vegetation capacity" based on soil, water, sunlight
- Species compete: trees shade out grass, grass outcompetes trees on thin soil
- Vegetation reduces erosion rate (multiply erosion by `1 - vegetation_cover`)
- Erosion removes soil, reducing vegetation capacity

### 3.2 Procedural Modeling of Plant Ecosystems Maximizing Vegetation Cover (2022)

**Source:** [Multimedia Tools and Applications](https://link.springer.com/article/10.1007/s11042-022-12107-8)

Treats plants as individual entities with rules for reproduction, growth, competition,
interaction, variation, and adaptation. Simulates ~500K plant models across 3 types (shrub,
conifer, deciduous). Predicts vegetation distribution patterns based on biological principles.

**Quality:** Solid journal paper.
**Our scale:** Our 256x256 grid = 65K cells. Running individual-based ecology for 3-5 species
is lightweight. Key parameters per species: growth rate, shade tolerance, water requirement,
soil preference, reproduction radius, competitive strength.

### 3.3 Procedural Urban Forestry (2022)

**Source:** [ACM Transactions on Graphics](https://dl.acm.org/doi/10.1145/3502220)

Simulates urban tree growth considering infrastructure constraints (buildings, roads, underground
pipes). Trees compete for light and water while respecting urban geometry.

**Quality:** Top-tier.
**Our scale:** Relevant when we add settlements. Trees + buildings = competition for space.

### 3.4 How Houdini Vegetation Scattering Works

Based on research into SideFX's approach:

Houdini uses a multi-pass scattering framework:
1. **Terrain analysis:** Compute slope, aspect, moisture, altitude per point
2. **Density maps:** Per-species probability fields based on terrain attributes
3. **Poisson disk sampling:** Place instances with minimum spacing constraints
4. **Competition pass:** Remove instances that overlap or violate proximity rules
5. **Adaptation:** Adjust instance scale/orientation based on local conditions

The ecosystem simulator (available as SideFX Labs tools) adds:
- Species-species interaction (shade, root competition)
- Temporal growth simulation
- Light-dependent growth direction

**Our scale:** We can implement the core loop: density maps -> sampling -> competition.
Skip the 3D geometry, just track species type and biomass per cell.

---

## 4. Geological Simulation

### 4.1 Simulating Worlds on the GPU: Four Billion Years in Four Minutes (davidar, 2021)

**Source:** [davidar.io/post/sim-glsl](https://davidar.io/post/sim-glsl)
**HN Discussion:** [news.ycombinator.com](https://news.ycombinator.com/item?id=27950641)

An entire Earth simulation in GLSL fragment shaders. The pipeline:

1. **Terrain:** 5-layer crater generation + fBm
2. **Tectonics:** Plates grow via diffusion-limited aggregation, discrete pixel-step movement,
   collisions raise elevation, thermal erosion spreads changes
3. **Hydraulic erosion:** Stream power law: `elevation -= 0.05 * pow(water, 0.8) * pow(slope, 2.0)`
4. **Climate:** MSLP from land/ocean + latitude sinusoids, Gaussian blur (10-15 deg std),
   temperature via tanh latitude + pressure + seasons, wind from Coriolis + pressure gradients
5. **Ecology:** Lotka-Volterra for vegetation/herbivores/predators with diffusion
6. **Civilization:** Settlement patterns based on resource access

All at 60fps in a single GLSL pass. No pre-rendered textures.

**Quality:** Brilliant hack. Not physically rigorous but captures the essential feedback loops.
**Our scale:** THIS IS THE CLOSEST EXISTING WORK TO WHAT WE'RE BUILDING. The key difference:
we run on CPU with a richer simulation model. Their climate approach (MSLP + Coriolis) is
exactly the right level of simplification for games. Their Lotka-Volterra ecology is a perfect
fit for our ant-colony-scale population dynamics.

### 4.2 Procedural Tectonic Planets (Cordonnier et al., 2019)

**Source:** [hal.science/hal-02136820](https://hal.science/hal-02136820/file/2019-Procedural-Tectonic-Planets.pdf)

Captures fundamental tectonic phenomena in a procedural method: plate subduction, collisions,
lithosphere deformation. Users control plate movement, which dynamically generates continents,
oceanic ridges, mountain ranges, island arcs.

**Quality:** High (research paper with solid geophysics basis).
**Our scale:** We could implement simplified plate tectonics as a pre-generation step. At
256x256, Voronoi-based plates with collision/subduction rules would generate realistic
continental shapes in seconds.

### 4.3 World Orogen -- Procedural Planet Generator

**Source:** [orogen.studio](https://www.orogen.studio/)

A browser-based tool (JavaScript/Three.js) with a remarkably complete pipeline:

**Tectonics:** Farthest-point seed placement with jitter, round-robin flood fill, directional
growth bias, compactness penalty, boundary smoothing, fragment reconnection. ~20 "super plates"
for broad orogenic belts.

**Erosion pipeline (multi-pass):**
1. Domain warping for organic coastlines
2. Bilateral smoothing
3. Glacial erosion (fjords, U-shaped valleys)
4. Priority-flood canyon carving (all land drains to ocean)
5. Iterative stream-power hydraulic erosion + deposition
6. Thermal erosion
7. Ridge sharpening
8. Soil creep

**Climate:** Blended dual-model precipitation (6 mechanisms including ITCZ convection and
orographic effects, mixed 50-50 with smooth heuristic zonal model). Seasonal wind patterns,
longitude-varying ITCZ, ocean currents with western boundary intensification, Koppen
classification.

49 stars, ~30 focused modules, MIT-ish license, no build dependencies.

**Quality:** Exceptional for a solo project. The climate model is more sophisticated than most
academic papers in this space.
**Our scale:** GOLD MINE FOR REFERENCE. Their erosion pipeline order and climate dual-model
approach are directly applicable. The Koppen classification lookup is exactly what we need for
biome assignment.

### 4.4 Astrolith: Procedural Planet Simulation

**Source (blog series):**
- [Rock Layers](https://www.gamedev.net/blogs/entry/2284060-rock-layers-for-real-time-erosion-simulation/)
- [Planet-scale Erosion](https://www.gamedev.net/blogs/entry/2275217-real-time-planet-scale-erosion/)
- [Snow Accumulation](https://www.gamedev.net/blogs/entry/2277860-realistic-snow-accumulation-in-erosion-simulator/)
- [Impact Craters](https://www.gamedev.net/blogs/entry/2277568-planet-generation-impact-craters/)

Clever rock layer system: predefined global layers with thickness + material type. Instead of
voxels, a linear SIMD-accelerated scan finds which layer intersects the elevation surface.
**Real-time: 10ms per 64x64 tile on CPU, no voxels.**

Each layer has its own erosion intensity, enabling **differential erosion** (soft layers erode
faster, creating realistic layered cliff faces).

**Quality:** Excellent engineering blog. Practical, performance-focused.
**Our scale:** DIRECTLY APPLICABLE. We could define 5-10 rock layer types with different
erosion rates. At 256x256, the layer lookup is trivial. This would make our terrain much more
geologically interesting -- granite mountains with sandstone valleys.

### 4.5 Karst Cave Generation

**Source:** [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0097849321002132)
**Geologically coherent caves:** [Computer Graphics Forum 2021](https://onlinelibrary.wiley.com/doi/abs/10.1111/cgf.14420)

Karst networks computed via gridless anisotropic shortest-path algorithms, considering inlets,
outlets, faults, inception horizons, fractures, permeability. Geometry defined as signed
distance function construction trees with blending and warping.

An alternative approach uses L-systems to emulate crack/passage formation, combined with
noise-perturbation and cellular automata.

**Quality:** Solid research.
**Our scale:** Cave systems could be a 2D cross-section on our grid. An L-system approach
generating cave networks based on water table level and limestone presence is feasible.
Could be a future feature for underground exploration gameplay.

### 4.6 Clustered Convection for Procedural Plate Tectonics (Nick McDonald, 2020)

**Source:** [nickmcd.me](https://nickmcd.me/2020/12/03/clustered-convection-for-simulating-plate-tectonics/)

Nick's approach to plate tectonics using clustered convection cells. Mantle convection drives
plate motion; plates are emergent from the convection pattern rather than manually defined.

**Quality:** High (well-documented, open implementation).
**Our scale:** Interesting alternative to Voronoi plates. Emergent plates from convection would
give more natural shapes. CPU-feasible at our scale.

---

## 5. Atmosphere and Ocean

### 5.1 Simplified GCM for Games: The Hadley Cell Approach

**Source:** [Jasper McChesney - Climate Modeling 101](https://medium.com/universe-factory/climate-modeling-101-4544e00a2ff2)

The practical framework for game climate:

**Atmospheric circulation:**
- 3 Hadley cells per hemisphere (tropical, mid-latitude, polar)
- 7 major climatic zones from cell boundaries
- Cell size depends on planetary radius and rotation speed
- Surface winds deflected by Coriolis force

**Orographic precipitation:**
- Significant when average relief > 1km
- Peak rainfall at 1-1.5km relief on windward side
- Rain shadow desert when relief > 2km on leeward side
- Even low mountains cause orographic lift with consistent winds

**Temperature model:**
- Latitude-based baseline (tanh curve)
- Altitude lapse rate (~6.5C per 1000m)
- Continental vs maritime moderation
- Seasonal variation

**Quality:** Excellent practical guide for worldbuilders.
**Our scale:** IMPLEMENT THIS. Our 256x256 grid can have latitude bands + prevailing wind
direction per band. Orographic precipitation from mountains blocking wind. Temperature from
latitude + altitude. This gives us realistic biome distribution cheaply.

### 5.2 Joe Duffy's Climate Simulation

**Source:** [joeduffy.games](https://www.joeduffy.games/climate-simulation-for-procedural-world-generation)

Practical implementation:
- Perlin noise landmass with falloff for ocean borders
- Temperature/precipitation from noise + latitude curve
- Biome from lookup table (temperature x precipitation)
- Rivers via downhill flow from Poisson-sampled mountain sources

**Quality:** Simple but effective tutorial implementation.
**Our scale:** A good starting point, but we should go further with wind simulation and rain
shadows rather than just noise-based precipitation.

### 5.3 Worldbuilding Pasta: Climate with ExoPlaSim

**Source:** [worldbuildingpasta.blogspot.com](https://worldbuildingpasta.blogspot.com/2020/05/an-apple-pie-from-scratch-part-vib.html)
**ExoPlaSim supplement:** [Part VI Supplement](https://worldbuildingpasta.blogspot.com/2021/11/an-apple-pie-from-scratch-part-vi.html)

Extremely detailed guide to climate worldbuilding. Covers:
- Koppen classification in depth
- Ocean current formation (thermohaline circulation, Ekman transport)
- Monsoon mechanics (land-sea temperature differential driving seasonal wind reversal)
- ITCZ migration with seasons
- Continental interior aridity
- Rain shadow quantification

Also covers using ExoPlaSim (a real GCM) for fictional planet climate simulation.

**Quality:** The most thorough worldbuilding climate resource available.
**Our scale:** Reference material for our climate model. We don't need ExoPlaSim's complexity,
but understanding the physics helps us choose the right simplifications.

### 5.4 Ocean Current Simulation for Games

Key principles extractable from the research:

- **Western boundary intensification:** Ocean currents are stronger on the western sides of
  ocean basins (Gulf Stream, Kuroshio). Simple to model: bias current strength westward.
- **Thermohaline circulation:** Deep water forms at poles (cold, salty), flows toward equator.
  Affects surface temperature patterns.
- **Upwelling zones:** Where winds push surface water away from coast, cold deep water rises.
  Creates foggy, cool, productive fishing zones (think Pacific coast).

**Our scale:** A simplified 2D ocean current model using wind-driven Ekman transport on our
grid is feasible. Even a static current map (computed once from wind patterns and coastline
geometry) would improve our climate model significantly.

### 5.5 NeuralGCM (Google Research, 2024)

**Source:** [research.google/blog/neuralgcm](https://research.google/blog/fast-accurate-climate-modeling-with-neuralgcm/)
**Paper:** [PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC11357988/)

Hybrid approach: physics-based GCM backbone with neural network parameterizations replacing
traditional sub-grid models. Faster and more accurate than pure-physics or pure-ML approaches.

**Quality:** Top-tier (Google Research / Nature).
**Our scale:** Way too heavy for us, but the architectural insight is relevant: use physics for
large-scale dynamics, learned models for sub-grid phenomena. We could apply this principle with
lookup tables instead of neural nets.

---

## 6. Procedural Cities and Roads

### 6.1 Tensor Field Road Networks (Chen & Esch, 2008)

**Source:** [sci.utah.edu](https://www.sci.utah.edu/~chengu/street_sig08/street_sig08.pdf)

The gold standard for procedural road networks. A tensor field is a 2D rotation matrix at each
point whose eigenvectors define two perpendicular street directions. Multiple tensor fields
(radial around landmarks, grid-aligned, terrain-following) are blended by weighted average.
Streets traced as hyperstreamlines.

**Quality:** Foundational (SIGGRAPH 2008). Still the best approach.
**Our scale:** A simplified version: define a few "attractor" tensors (town centers create
radial fields, terrain slope creates contour-following fields, major routes create directional
fields). Trace roads through the blended field. At 256x256, this is cheap.

### 6.2 Procedural Villages on Arbitrary Terrains (Galin et al., 2012)

**Source:** [The Visual Computer](https://link.springer.com/article/10.1007/s00371-012-0699-7)
**PDF:** [inria.hal.science/hal-00694525](https://inria.hal.science/hal-00694525v1)

Settlement generation using **interest maps** to progressively place settlement seeds and roads.
New roads attract settlers; new houses extend road networks. Anisotropic conquest segments
land into parcels. Open shape grammar generates 3D geometry adapting to local slope.

**Quality:** Solid research, directly game-applicable.
**Our scale:** HIGHLY RELEVANT for our settlement system. Interest maps combine terrain
suitability (flat, near water, fertile soil) with existing infrastructure (roads, other
settlements). Iterative growth matches our simulation-over-time approach.

### 6.3 Semantically Plausible Small-Scale Towns (2023)

**Source:** [ScienceDirect](https://www.sciencedirect.com/science/article/pii/S1524070323000012)

Procedural organic settlement generation based on community necessities. Generates natural-
looking small-scale residential regions similar to pre-industrial era cities. Uses radial grid
with deformation techniques for heterogeneous growth patterns.

**Quality:** Recent, practical.
**Our scale:** Pre-industrial settlements are exactly what we need. The radial growth model
with terrain deformation is simple to implement.

### 6.4 Tensor Field + Multi-Agent Road Networks (2022)

**Source:** [ResearchGate](https://www.researchgate.net/publication/364451549)

Combines tensor field road generation with multi-agent simulation. Agents represent different
types of road users (pedestrians, vehicles, trade routes), and their movement patterns
influence road placement.

**Quality:** Interesting hybrid approach.
**Our scale:** Aligns perfectly with our ant-colony AI vision. Agent movement paths naturally
form roads. Roads should emerge from traffic, not be placed manually (per user feedback).

### 6.5 CityEngine CGA Shape Grammar

**Source:** [doc.arcgis.com](https://doc.arcgis.com/en/cityengine/latest/get-started/get-started-about-cityengine.htm)

CGA (Computer Generated Architecture) is a rule-based system where buildings are generated
by recursive subdivision of volumes. Rules encode architectural styles. Combined with L-system
road networks and image-map inputs (population density, land use, max building height).

**Quality:** Industry standard (Esri).
**Our scale:** We don't need 3D building generation, but the concept of rule-based building
placement based on zoning maps is useful. Define building types with rules: "blacksmith needs
road access + flat terrain + within 50m of town center."

### 6.6 Terasology Cities Module

**Source:** [github.com/Terasology/Cities](https://github.com/Terasology/Cities)

Open-source settlement generator: finds suitable locations per sector, places settlements of
varying sizes, connects with long-distance roads, generates lots and local streets.

**Quality:** Game-ready, but basic.
**Our scale:** Good reference for the overall pipeline: site selection -> road connection ->
lot subdivision -> building placement.

---

## 7. Hidden Gem Repos

### 7.1 SoilMachine (weigert/Nick McDonald)

**Source:** [github.com/weigert/SoilMachine](https://github.com/weigert/SoilMachine)
**Blog post:** [nickmcd.me/2022/04/15/soilmachine](https://nickmcd.me/2022/04/15/soilmachine/)

The most sophisticated open-source terrain erosion simulator. Uses a **run-length encoded
doubly-linked list** at each grid position for layered terrain. Each section stores height,
soil type, saturation, pointers to adjacent layers.

Why it's better than voxels: maintains continuous height (critical for slope computation),
dynamically allocates only needed layers, memory-efficient.

Erosion via generalized particle base class with water/wind derivatives. Sediment conversion
graph handles soil transformations (massive rock -> gravel -> fine sediment). Water placement
via local cellular automaton cascading instead of expensive flood fills.

Rendering: vertex pooling (never remeshes). C++ with TinyEngine, MIT license.

**Stars:** ~200. **Quality:** Exceptional engineering.
**Our scale:** The layered data structure idea is gold. We could use 3-5 layers per cell
(bedrock, subsoil, topsoil, sediment, vegetation) without the full linked-list complexity.
The particle erosion with soil-type-specific parameters is directly implementable.

### 7.2 SimpleWindErosion (weigert)

**Source:** [github.com/weigert/SimpleWindErosion](https://github.com/weigert/SimpleWindErosion)

Particle-based wind erosion: abrasion, suspension, cascading, aeolian processes. Clean,
minimal implementation.

**Quality:** Excellent reference implementation.
**Our scale:** Wind erosion would add sand dunes, wind-sculpted rock formations. Simple to add
alongside our existing hydraulic erosion.

### 7.3 davidar's GLSL Planet Sim

**Source:** [davidar.io/post/sim-glsl](https://davidar.io/post/sim-glsl)

(Detailed in Section 4.1 above.) Single GLSL shader simulating 4 billion years at 60fps.
Tectonics + erosion + climate + ecology + civilization.

**Stars:** Not a repo per se (Shadertoy-style), but the blog post is a masterclass.
**Quality:** Brilliant. The closest thing to "all of Earth science in 200 lines."

### 7.4 World Orogen

**Source:** [github.com related to orogen.studio](https://www.orogen.studio/)

(Detailed in Section 4.3 above.) 49 stars. JavaScript/Three.js. Full tectonic + erosion +
climate pipeline. 30 focused modules. No build dependencies.

**Quality:** Exceptional for a web tool. The climate model is genuinely good.

### 7.5 WorldMachina

**Source:** [github.com/SAED2906/WorldMachina](https://github.com/SAED2906/WorldMachina)

Python/OpenGL planet generator with tectonic simulation, height-based displacement, ocean
rendering. Small project but clean implementation.

**Quality:** Moderate. Good learning resource.

### 7.6 planet_heightmap_generation

**Source:** [github.com/raguilar011095/planet_heightmap_generation](https://github.com/raguilar011095/planet_heightmap_generation)

Browser-based procedural planet with tectonics, erosion, climate. JavaScript. Interactive
editing.

**Quality:** Moderate. Good for quick experimentation.

### 7.7 Procedural-Tectonics (FioDev)

**Source:** [github.com/FioDev/Procedural-Tectonics](https://github.com/FioDev/Procedural-Tectonics)

Technology demonstration for procedural tectonic effects as a terrain generation engine.
Simulates plate interactions, convergent/divergent boundaries, volcanism.

**Quality:** Demo-level but interesting.

### 7.8 Realistic Planet Generation and Simulation (FreezeDriedMangos)

**Source:** [github.com/FreezeDriedMangos/realistic-planet-generation-and-simulation](https://github.com/FreezeDriedMangos/realistic-planet-generation-and-simulation)

Full pipeline: plate tectonics -> weather -> climate -> ocean currents.

**Quality:** Ambitious personal project.

### 7.9 terrain-erosion-3-ways (dandrino)

**Source:** [github.com/dandrino/terrain-erosion-3-ways](https://github.com/dandrino/terrain-erosion-3-ways)

Clean comparison of three erosion approaches in a single codebase. Good for understanding
tradeoffs between particle-based, grid-based, and hybrid methods.

**Stars:** ~700. **Quality:** Excellent teaching resource.

### 7.10 UnityTerrainErosionGPU (bshishov)

**Source:** [github.com/bshishov/UnityTerrainErosionGPU](https://github.com/bshishov/UnityTerrainErosionGPU)

Shallow water equation-based hydraulic + thermal erosion in Unity compute shaders. Pipe model
for water flow.

**Quality:** Good GPU reference implementation.

### 7.11 heightmap-erosion (rj00a)

**Source:** [github.com/rj00a/heightmap-erosion](https://github.com/rj00a/heightmap-erosion)

Parallel Rust implementation of erosion. "Unvectorized" but demonstrates Rust-native terrain
manipulation.

**Quality:** Small but relevant (Rust!).

---

## 8. Applicability to Our System

### Our constraints:
- 256x256 grid (~65K cells)
- CPU-only, terminal rendering
- 60fps target (simulation can amortize across frames)
- Rust implementation
- 7-stage pipeline, 14 biomes, settlement simulation

### Priority implementations (sorted by impact/effort ratio):

#### Tier 1: Implement Soon

| Technique | Source | Effort | Impact |
|-----------|--------|--------|--------|
| Stream Power Erosion | Schott 2023 | Medium | High -- more geologically correct than particle erosion at our scale |
| Hadley Cell Climate | McChesney / davidar | Low | High -- realistic biome distribution from simple rules |
| Orographic Precipitation | Multiple | Low | High -- rain shadows create desert/forest boundaries |
| Rock Layer Differential Erosion | Astrolith | Low | High -- 5-10 layer types with different erosion rates |
| Vegetation-Erosion Feedback | Cordonnier 2017 | Low | Medium -- vegetation stabilizes soil, erosion kills vegetation |
| "Fastest Erosion" Post-Process | Procedural Pixels | Low | Medium -- cheap visual enhancement |

#### Tier 2: Implement When Ready

| Technique | Source | Effort | Impact |
|-----------|--------|--------|--------|
| Debris Flow on Steep Slopes | Jain 2024 | Medium | Medium -- fixes unrealistic ridge erosion |
| Layered Soil Data Structure | SoilMachine | Medium | Medium -- enables subsurface modeling |
| Interest Map Settlements | Galin 2012 | Medium | High -- terrain-aware town placement |
| Agent-Driven Roads | Tensor + Multi-Agent | Medium | High -- roads emerge from traffic patterns |
| Wind Erosion | SimpleWindErosion | Medium | Medium -- sand dunes, aeolian features |
| LBM Shallow Water | Academic | High | Medium -- more accurate water flow |

#### Tier 3: Future Features

| Technique | Source | Effort | Impact |
|-----------|--------|--------|--------|
| Glacial Erosion (simplified) | Cordonnier 2023 | High | Medium -- fjords, U-valleys |
| Koppen Climate Classification | World Orogen | Medium | Medium -- more nuanced biomes |
| Ocean Currents | Multiple | High | Medium -- coastal climate effects |
| Cave Generation (L-system) | Academic | High | Low -- underground exploration |
| Tectonic Pre-Generation | Cordonnier 2019 | High | Medium -- realistic continental shapes |
| Unerosion (time reversal) | Yang 2024 | High | Low -- cool but niche |

### Key architectural insights:

1. **Stream Power > Particle Erosion at our scale.** The SPL equation
   `dz/dt = U - K * A^m * S^n` operates on the grid directly. No particles needed. Drainage
   area computation is the bottleneck but FastFlow shows O(log n) is achievable.

2. **Climate before erosion.** Our pipeline should compute prevailing winds + precipitation
   BEFORE running erosion, so erosion rates vary spatially (wet slopes erode faster).

3. **Layered materials make everything better.** Even 3 layers (bedrock, soil, sediment) with
   different erosion rates create vastly more realistic terrain than a single heightmap.

4. **Vegetation is a simulation, not a biome lookup.** Species competition + erosion feedback
   creates emergent biome boundaries rather than hard-coded thresholds.

5. **Roads from movement, not placement.** Agent paths wear terrain -> paths become roads ->
   roads attract settlement -> settlement generates more traffic. Emergent infrastructure.

6. **Multi-scale thinking.** Coarse simulation on the full grid, procedural detail at render
   time. The Phasor noise paper shows how to add convincing erosion detail without actually
   simulating it at high resolution.

---

## Appendix: Key Research Groups

These groups consistently produce state-of-the-art terrain work:

- **LIRIS Lyon (Galin, Guerin, Paris, Grenier):** Stream power erosion, multi-scale terrain,
  phasor noise enhancement, debris flow. The most prolific terrain research group.
- **Purdue CGVLab (Benes, Jain):** Debris flow, FastFlow, unerosion. Strong GPU focus.
- **Inria IMAGINE (Cani, Cordonnier):** Glacial erosion, tectonic planets, ecosystem authoring.
  Emphasis on geological realism.
- **Nick McDonald (weigert/nickmcd):** SoilMachine, wind erosion, procedural hydrology. Best
  open-source implementations.
- **davidar:** GLSL planet simulation. Proof that the whole pipeline fits in a shader.

---

*Last updated: 2026-04-04*
