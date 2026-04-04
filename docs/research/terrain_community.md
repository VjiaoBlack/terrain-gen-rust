# Terrain Generation Community Reference

A curated reference of people, papers, tools, and communities working on
procedural terrain with realistic erosion, hydrology, and related simulation.
Compiled April 2026.

---

## Nick McDonald / weigert — The Anchor

### SoilMachine
- **Author:** Nick McDonald (weigert)
- **URL:** https://nickmcd.me/2022/04/15/soilmachine/ | https://github.com/weigert/SoilMachine
- **What:** Full geomorphology simulator: multi-layer soil stacks, particle-based hydraulic + thermal + wind erosion, porous groundwater, sediment conversion graph.
- **Relevance to us:** The direct spiritual predecessor to everything we are building. Key advance over SimpleHydrology: the LayerMap — a 2D array of run-length-encoded doubly-linked soil section lists — gives true 3D subsurface at low memory cost. Each soil type has solubility, erosion rate, friction, and porosity parameters; soils convert to other types during erosion (e.g. rock → gravel). Surface water and subsurface saturation are unified. Vegetation was explicitly called out as future work. Wind particles derived from same base class as water particles.

### soillib
- **Author:** Nick McDonald (erosiv org)
- **URL:** https://github.com/erosiv/soillib
- **What:** C++23 + CUDA library for numerical geomorphology simulation on the GPU, with Python3 bindings via nanobind. GeoTIFF import/export, NumPy/PyTorch interop.
- **Relevance to us:** The mature, reusable distillation of SoilMachine's ideas. Kernelized GPU erosion operations. If we ever go GPU-accelerated for generation this is the reference architecture. Python bindings mean it can be scripted for batch world gen.

### SimpleHydrology / Procedural Hydrology
- **Author:** Nick McDonald
- **URL:** https://nickmcd.me/2020/04/15/procedural-hydrology/ | https://github.com/weigert/SimpleHydrology
- **What:** Particle-based river and lake simulation; the foundational public write-up of the approach.
- **Relevance to us:** Our hydrology stage is already inspired by this. Review for the meandering rivers improvements (2023 post on nickmcd.me).

### Meandering Rivers Improvements (2023)
- **Author:** Nick McDonald
- **URL:** https://nickmcd.me/2023/ (post: "Procedural Hydrology: Improvements and Meandering Rivers")
- **What:** Particle erosion improvements and physically-based meandering river behavior added to SimpleHydrology.
- **Relevance to us:** Direct input for our river meander pass. Check against Axel Paris's meandering rivers paper (below) for complementary ideas.

---

## Sebastian Lague

### Hydraulic Erosion (Coding Adventure)
- **Author:** Sebastian Lague
- **URL:** https://github.com/SebLague/Hydraulic-Erosion | https://sebastian.itch.io/hydraulic-erosion
- **What:** 70k particle droplet erosion simulation in Unity; highly accessible video/code tutorial.
- **Relevance to us:** The most widely-watched introduction to particle erosion. Our erosion stage should match or exceed this quality; good benchmark for visual output. MIT licensed C# implementation, readable for porting logic.

### Procedural Landmass Generation
- **Author:** Sebastian Lague
- **URL:** https://github.com/SebLague/Procedural-Landmass-Generation
- **What:** Full Unity terrain pipeline series — noise, falloff maps, chunk streaming, LOD.
- **Relevance to us:** Reference for chunked infinite terrain architecture; our settlement sim needs similar streaming.

---

## Hans Theobald Beyer

### Implementation of a Method for Hydraulic Erosion (Thesis)
- **Author:** Hans Theobald Beyer
- **URL:** https://www.firespark.de/?id=project&project=HydraulicErosion
- **What:** Bachelor thesis formalizing particle-based hydraulic erosion on heightmaps; became a widely-cited practical algorithm.
- **Relevance to us:** The Beyer algorithm is what most open-source implementations (including Lague's) are based on. Important reference for sediment capacity, deposition, and erosion brush mechanics. Minimum terrain height constraint prevents runaway drain valleys.

---

## Axel Paris (aparis69) — LIRIS / Adobe Research

### Learn Procedural Generation (Guide)
- **Author:** Axel Paris
- **URL:** https://aparis69.github.io/LearnProceduralGeneration/
- **What:** Free interactive web guide covering terrain noise, primitive generation, simulation, volumetric techniques; uses Three.js for live demos.
- **Relevance to us:** Best single pedagogical resource for the techniques in our pipeline. Good for onboarding contributors. Planned future sections on ecosystem + city gen.

### Terrain Amplification with Implicit 3D Features (SIGGRAPH Asia 2019)
- **Author:** Axel Paris, Eric Galin, Eric Guérin et al.
- **URL:** https://github.com/aparis69/Implicit-Volumetric-Terrains | https://dl.acm.org/doi/10.1145/3342765
- **What:** Heightfield terrains amplified with implicit 3D structures: slot canyons, sea arches, karst caves, hoodoos.
- **Relevance to us:** If we want cliffs, overhangs, or cave entrances that are geologically motivated, this is the paper. The implicit blending approach could augment our heightmap stages without requiring full voxel storage.

### Terrain Erosion on the GPU (Blog Post)
- **Author:** Axel Paris
- **URL:** https://aparis69.github.io/public_html/posts/terrain_erosion.html
- **What:** Implementation notes and code for GPU hydraulic erosion.
- **Relevance to us:** Practical GPU erosion reference; complements soillib.

### Authoring and Simulating Meandering Rivers (SIGGRAPH Asia 2023)
- **Author:** Axel Paris, Eric Guérin, Pauline Collon, Eric Galin
- **URL:** https://github.com/aparis69/Meandering-rivers | https://dl.acm.org/doi/10.1145/3618350
- **What:** Physically-based meandering simulation: curvature-driven bend migration, cutoffs, oxbow lakes, avulsions.
- **Relevance to us:** Direct target for our river system upgrade. Source code available. Oxbow lakes and cutoffs are exactly the kind of geologically interesting features that make terrain feel ancient.

---

## Hugo Schott — LIRIS / Lyon

### Stream Power Erosion (Open Source)
- **Author:** Hugo Schott
- **URL:** https://github.com/H-Schott/StreamPowerErosion | https://h-schott.github.io/publications/uplift/publi_uplift.html
- **What:** User paints uplift map; fluvial erosion via the Stream Power Equation from geomorphology produces dendritic mountainous terrain with correct drainage networks.
- **Relevance to us:** The uplift-domain approach lets us set large-scale mountain/valley structure then let physics determine the detail. Much more geologically honest than painting elevation. Real-time interactive on GPU.

### Large-Scale Terrain Authoring through Interactive Erosion Simulation (ACM ToG 2023)
- **Author:** Hugo Schott, Axel Paris, Eric Guérin, Eric Galin et al.
- **URL:** https://dl.acm.org/doi/10.1145/3592787 | https://hal.science/hal-04049125
- **What:** Interactive uplift-domain terrain authoring: copy-paste, warp/fold/fault operations, elevation constraints — all driving real-time stream-power erosion.
- **Relevance to us:** The authoring paradigm we should adopt for world-editor tooling. "Paint intent, get geology."

### Terrain Amplification using Multi-Scale Erosion (ACM ToG 2024)
- **Author:** Hugo Schott, Eric Galin, Eric Guérin, Axel Paris, Adrien Peytavie
- **URL:** https://dl.acm.org/doi/10.1145/3658200 | https://github.com/H-Schott/MultiScaleErosion
- **What:** Amplifies low-resolution terrain to high-resolution hydrologically consistent detail using multi-scale thermal, stream power, and deposition passes.
- **Relevance to us:** Our pipeline currently works at a single resolution. This multi-scale approach would let us generate continental-scale structure then zoom into regional detail without regenerating from scratch.

---

## Guillaume Cordonnier — Inria / ENS Lyon

### Large Scale Terrain Generation from Tectonic Uplift and Fluvial Erosion (CGF 2016)
- **Author:** Guillaume Cordonnier, Jean Braun, Marie-Paule Cani, Bedrich Benes, Eric Galin et al.
- **URL:** https://www.cs.purdue.edu/cgvlab/www/resources/papers/Cordonnier-Computer_Graphics_Forum-2016-Large_Scale_Terrain_Generation_from_Tectonic_Uplift_and_Fluvial_.pdf
- **What:** First CG paper combining tectonic uplift with the geologist's Stream Power Equation to generate geologically plausible mountain ranges with correct watershed hierarchies.
- **Relevance to us:** Foundational. Our tectonic plate stage should be informed by this — stream graphs over the full domain enable correct mountain ridge / valley / watershed topology.

### Forming Terrains by Glacial Erosion (SIGGRAPH 2023)
- **Author:** Guillaume Cordonnier, Guillaume Jouvet, Adrien Peytavie et al.
- **URL:** https://dl.acm.org/doi/10.1145/3592422 | https://inria.hal.science/hal-04090644
- **What:** Deep-learning estimated high-order ice flows + multi-scale advection produces U-shaped valleys, hanging valleys, fjords, glacial lakes, and cliff debris.
- **Relevance to us:** If our world has tundra/arctic biomes, glaciated terrain is geologically distinct from river-eroded terrain. This gives the technique for differentiating them correctly.

---

## Bedrich Benes — Purdue University

### Visual Simulation of Hydraulic Erosion (2002, foundational)
- **Author:** Bedrich Benes, Rafael Forsbach
- **URL:** https://www.cs.purdue.edu/cgvlab/www/resources/papers/Benes-2002-Visual_simulation_of_hydraulic_erosion.pdf
- **What:** Grid-based (pipe model) hydraulic erosion — the origin point of much subsequent work. Layered data representation for 3D terrains.
- **Relevance to us:** Historical context. Understanding the pipe model vs. particle model trade-offs.

### Authoring Landscapes by Combining Ecosystem and Terrain Erosion Simulation (SIGGRAPH 2017)
- **Author:** Guillaume Cordonnier, Eric Galin, James Gain, Bedrich Benes, Eric Guérin, Adrien Peytavie, Marie-Paule Cani
- **URL:** https://dl.acm.org/doi/10.1145/3072959.3073667
- **What:** Bi-directional feedback between vegetation and erosion simulation: rock, sand, humus, grass, shrubs, trees all interact with erosion agents and each other.
- **Relevance to us:** The definitive paper on coupled vegetation-terrain simulation. Validates vegetation influencing erosion (root binding, canopy interception) and erosion influencing vegetation distribution. Target architecture for our biome-terrain feedback layer.

### Efficient Debris-flow Simulation for Steep Terrain Erosion (ACM ToG 2024)
- **Author:** Purdue CGVLab (Arymaan, Benes et al.)
- **URL:** https://dl.acm.org/doi/10.1145/3658213 | https://www.cs.purdue.edu/cgvlab/www/resources/papers/Arymaan-ToG-2024-efficient.pdf
- **What:** GPU algorithm unifying debris-flow erosion (mud + rock mixture) with fluvial erosion; derived from real geomorphology equations.
- **Relevance to us:** Steep mountain biomes should have debris fans and alluvial cones. This is the technique.

### Unerosion: Simulating Terrain Evolution Back in Time (SCA 2024)
- **Author:** Yang, Cordonnier, Cani, Perrenoud, Benes
- **URL:** https://dl.acm.org/doi/10.1111/cgf.15182 | https://www.cs.purdue.edu/cgvlab/www/resources/papers/Yang-CGF-2024-Unerosion.pdf
- **What:** Reverse erosion — recover plausible past topographies from current terrain by running fluvial, sediment, and thermal erosion backward in time.
- **Relevance to us:** Fascinating for world history / lore generation ("what did this valley look like 10,000 years ago?"). Could drive geological age markers in our layer system.

---

## Inigo Quilez

### Terrain Raymarching & Procedural Noise Articles
- **Author:** Inigo Quilez (iquilezles)
- **URL:** https://iquilezles.org/articles/terrainmarching/
- **What:** Foundational articles on raymarched terrain, fBm noise, domain warping, and smooth analytic SDFs.
- **Relevance to us:** Our rendering stage. Distance-field terrain marching could replace heightmap rasterization for terminal rendering; domain warping gives erosion-like appearance for "free" at the noise level.

---

## Shadertoy Community

### Advanced Terrain Erosion Filter (Shadertoy)
- **Author:** runevision (Rune Skovbo Johansen)
- **URL:** https://www.shadertoy.com/view/wXcfWn
- **What:** GPU-friendly erosion appearance filter — not a simulation, but produces branching gullies and ridges evaluable per-point, usable as a noise layer on any height function.
- **Relevance to us:** Zero-cost erosion appearance for distant terrain or low-priority chunks. The filter can be layered on top of our existing pipeline as a post-process.

### Fast and Gorgeous Erosion Filter (Blog)
- **Author:** runevision (Rune Skovbo Johansen)
- **URL:** https://blog.runevision.com/2026/03/fast-and-gorgeous-erosion-filter.html
- **What:** March 2026 writeup of an improved erosion noise technique with correct derivative math, more intuitive parameters, and crispier gullies; MPL v2 licensed.
- **Relevance to us:** Latest state-of-the-art non-simulation erosion appearance. MPL license means we can use and improve it. Consider as a Stage 3 enhancement.

### Clean Terrain Erosion Filter
- **Author:** runevision
- **URL:** https://www.shadertoy.com/view/33cXW8
- **What:** Rewrite of the Advanced Erosion Filter with corrected derivative math; cleaner reference implementation.
- **Relevance to us:** Study the implementation before porting erosion-appearance logic to Rust.

---

## Academic Papers (Standalone Entries)

### Physically-Based Analytical Erosion for Fast Terrain Generation (CGF 2024)
- **Author:** Tzathas, Galin, Guérin et al.
- **URL:** https://onlinelibrary.wiley.com/doi/10.1111/cgf.15033 | http://www-sop.inria.fr/reves/Basilic/2024/TGSC24/Analytical_Terrains_EG.pdf
- **What:** Analytical solutions of the Stream Power Law rather than time-stepped simulation; fast, consistent large-scale terrain generation without iterative convergence.
- **Relevance to us:** Could make our mountain generation stage much faster — solve for equilibrium directly rather than running N erosion steps.

### Flexible Terrain Erosion (The Visual Computer 2024)
- **Author:** Faraj, Galin et al.
- **URL:** https://link.springer.com/article/10.1007/s00371-024-03444-w | https://www.lirmm.fr/~nfaraj/publications/flexible_erosion/2024_Flexible_Terrain_Erosion.pdf
- **What:** Procedurally adds ravine/gully patterns at arbitrary scales and orientations based on local terrain characteristics, in real time.
- **Relevance to us:** Fast regional erosion detail that adapts to our existing heightmap. Could post-process our Stage 3 to add terrain-specific gully density.

### Terrain Generation Using Procedural Models Based on Hydrology (SIGGRAPH 2013)
- **Author:** Jean-David Génevaux, Eric Galin, Eric Guérin, Adrien Peytavie, Bedrich Benes
- **URL:** https://dl.acm.org/doi/10.1145/2461912.2461996
- **What:** Rivers as modeling elements; hierarchical drainage networks as geometric graphs drive terrain shape.
- **Relevance to us:** Foundational hydrology-first terrain paper. Our river placement approach should derive drainage hierarchy from terrain, not the reverse — this paper formalizes why.

---

## Open Source Implementations

### terrain-erosion-3-ways
- **Author:** Daniel Andrino (dandrino)
- **URL:** https://github.com/dandrino/terrain-erosion-3-ways
- **What:** Python implementations of three erosion approaches: simulated hydraulic (particle), river network graph, and machine learning.
- **Relevance to us:** The river network approach (O(N² log N)) is particularly interesting as an alternative to full particle simulation for rapid layout passes. ML approach suggests training data for learned terrain features.

### UnityTerrainErosionGPU
- **Author:** Boris Shishov (bshishov)
- **URL:** https://github.com/bshishov/UnityTerrainErosionGPU
- **What:** Hydraulic and thermal erosion using shallow-water equations, GPU compute shaders in Unity. Inspired by From Dust.
- **Relevance to us:** Shallow water equations give more physically correct wave/flow behavior than pure particle erosion. Reference architecture for a future GPU erosion pass.

### heightmap-erosion (Rust)
- **Author:** rj00a
- **URL:** https://github.com/rj00a/heightmap-erosion
- **What:** Rust port of Daniel Andrino's erosion simulation; parallel implementation.
- **Relevance to us:** Direct Rust reference. Study before expanding our own erosion stage — don't reinvent what's already idiomatic Rust.

### Kosmos (Rust + WebGPU)
- **Author:** kaylendog
- **URL:** https://github.com/kaylendog/kosmos
- **What:** Modular procedural terrain generator in Rust + WebGPU; node-graph pipeline like World Machine; includes hydraulic erosion, thermal erosion, voxel caves, LOD.
- **Relevance to us:** The closest Rust project to our own architecture. Study their node graph design for pipeline composability. GPL-3.0 but generated terrain is commercially usable.

### go_gens (Go)
- **Author:** Flokey82
- **URL:** https://github.com/Flokey82/go_gens
- **What:** Large collection of Go procedural generation experiments including erosion (port of Nick McDonald's work), voxel terrain, isometric rendering, story gen, and more.
- **Relevance to us:** Go's simplicity makes these easy to read as algorithm references. The hydrology port closely follows SimpleHydrology and may contain bug fixes or clarifications.

---

## Professional Tools (Reference Architecture)

### World Machine
- **Author:** Stephen Schmitt / World Machine Software
- **URL:** https://www.world-machine.com/
- **What:** Node-graph terrain generator; industry standard for AAA game terrain; flow erosion, thermal erosion, snow, rivers.
- **Relevance to us:** The gold standard for quality output. When evaluating our results, World Machine output is the benchmark. Its feature set defines what "complete" looks like.

### Gaea (QuadSpinner)
- **Author:** Dax Pandhi / QuadSpinner
- **URL:** https://quadspinner.com/
- **What:** Artist-friendly terrain authoring with directed erosion, ecosystem design; used in Death Stranding 2.
- **Relevance to us:** Gaea's "directed erosion" — painting erosion strokes onto 3D geometry — is an inspiration for our future world editor brush system.

### Instant Terra (Wysilab)
- **Author:** Wysilab
- **URL:** https://www.wysilab.com/
- **What:** Real-time terrain tool; flow maps, sediment deposition simulation; fast iteration for game dev.
- **Relevance to us:** Lightweighter alternative for players who want faster iteration; their flow-map generation approach could inform our moisture/drainage stage.

---

## ASCII / Terminal Terrain Rendering

### Terrain in the Terminal
- **Author:** Perry (perrycode.com)
- **URL:** https://perrycode.com/2015/05/28/termterrain/
- **What:** Diamond-square heightmap rendered to terminal with ncurses using colored ASCII characters; values 0.0–1.0 mapped to character set.
- **Relevance to us:** Direct reference for our work-mode ASCII renderer. The key insight: character selection + color together gives more visual depth than either alone.

### 2D Top Down ASCII Procedural Terrain Gen (GameDev.net thread)
- **URL:** https://www.gamedev.net/forums/topic/635083-2d-top-down-ascii-procedural-terrain-gen/
- **What:** Community discussion of ASCII terrain rendering techniques for top-down games.
- **Relevance to us:** Practical tips on character-to-terrain-type mapping from practitioners who actually shipped ASCII terrain games.

---

## Communities and Ongoing Resources

### r/proceduralgeneration
- **URL:** https://reddit.com/r/proceduralgeneration
- **What:** Active subreddit for procedural generation work; frequent terrain erosion showcases.
- **Relevance to us:** Monitor for new techniques, implementations, and demos. Post our own work here for community feedback.

### Shadertoy (terrain tag)
- **URL:** https://www.shadertoy.com/
- **What:** Browser-based GPU shader sandbox; large collection of terrain demos, erosion filters, and noise experiments.
- **Relevance to us:** Best place to prototype visual ideas before implementing in Rust. Search "terrain erosion" for the active community of filter developers.

### Axel Paris Publication List
- **URL:** https://aparis69.github.io/public_html/publications.html
- **What:** Curated list of all papers from the LIRIS terrain group; the most productive academic group in this space.
- **Relevance to us:** Subscribe mentally; check quarterly. Every new paper from this group is likely relevant.

---

## Key People to Follow

| Person | Handle / Affiliation | Focus |
|--------|---------------------|-------|
| Nick McDonald | @weigert (GitHub) | Particle erosion, soillib, SoilMachine |
| Axel Paris | @aparis69 (GitHub), Adobe Research | Implicit terrains, meandering rivers, erosion |
| Hugo Schott | @H-Schott (GitHub), LIRIS Lyon | Stream power erosion, uplift authoring |
| Guillaume Cordonnier | Inria | Tectonics + erosion, glacial erosion |
| Bedrich Benes | Purdue CGVLab | Vegetation + erosion coupling, debris flow |
| Eric Galin | LIRIS / CNRS | Noise, implicit features, terrain authoring |
| Eric Guérin | LIRIS / INSA Lyon | Procedural erosion patterns, amplification |
| Inigo Quilez | @iquilezles | Raymarching, noise, SDF terrain |
| Sebastian Lague | @SebLague (YouTube/GitHub) | Accessible erosion tutorials |
| Rune Skovbo Johansen | runevision | GPU erosion filters, Godot/Unity tools |
| Felix Westin | @Fewes / @FewesW | TerrainPrettifier, GPU noise (Unity) |

---

*Last updated: April 2026. Add new entries as discovered.*
