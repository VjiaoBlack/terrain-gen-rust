# Deep Research Prompt

Copy-paste this into ChatGPT Deep Research, Perplexity, or similar:

---

I'm building a terminal-based settlement simulation game in Rust with procedural terrain generation. The game uses a 256x256 tile grid with Perlin noise (fBm) for height generation, basic hydraulic erosion (water flows downhill, erodes terrain), and elevation-based biome assignment (water < sand < grass < forest < mountain < snow).

I need practical, implementable answers to improve my terrain generation. For each topic below, I want:
1. **The algorithm** — pseudocode or step-by-step description I can implement
2. **Key parameters** — what values to start with and how to tune them
3. **How games do it** — specific examples from Dwarf Fortress, Songs of Syx, Minecraft, Banished, Rimworld, or similar
4. **Pitfalls** — common failure modes and how to avoid them (e.g., rivers that are 1 pixel wide, erosion that makes everything flat)

## Topics

### 1. Cliff Generation
How to create sharp cliff faces and terraced plateaus in a 2D heightmap. I want mountains with flat tops and steep edges, not smooth bumps. Techniques: terrace functions, thermal weathering erosion, slope-based terrain type assignment. How to avoid thin spiky artifacts.

### 2. Moisture-Driven Biomes
How to distribute vegetation based on moisture + temperature instead of just elevation. I want forests near rivers, scrubland in dry areas, marsh in low wet areas. The Whittaker biome diagram approach. Simple moisture diffusion on a tile grid. Rain shadow effects from mountains.

### 3. Soil Quality Model
How to model soil fertility for farming. The USDA soil texture triangle (sand/silt/clay). How soil type correlates with terrain: alluvial silt near rivers, sandy near coast, clay on plains, rocky near mountains. How soil affects farm yield and water drainage. Keep it simple — 3-5 soil types max.

### 4. Better Hydraulic Erosion
My current erosion creates thin 1-tile channels. I want wider natural-looking valleys and riverbeds. Particle-based erosion (Sebastian Lague style). Sediment capacity based on water speed. Parameters to control minimum river width. How to create flat valley floors with erosion.

### 5. River Generation
How to procedurally generate connected river networks. Start from mountains, path to ocean following terrain gradient. How to make rivers 2-5 tiles wide. The drainage basin approach. How to combine deliberate river paths with erosion simulation.

### 6. Groundwater & Springs
Simple groundwater model for a tile grid game. Water percolating down, pooling at impermeable layers. Springs where hillside meets water table. How Dwarf Fortress handles aquifers. Using groundwater level to affect vegetation and farm yields.

## Constraints
- 256x256 tile grid, needs to generate in < 2 seconds
- Rust implementation, no GPU shaders
- Top-down 2D rendering (not 3D), so visual cues for height differences matter
- Must integrate with existing Perlin noise heightmap
- Game runs at 30-60fps, so ongoing simulation must be lightweight
