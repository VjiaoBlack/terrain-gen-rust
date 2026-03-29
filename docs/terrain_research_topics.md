# Terrain Generation Research Topics

Research questions for improving terrain-gen-rust's procedural world generation. These are areas where internet deep research would be valuable — the goal is to find practical algorithms and parameters, not just theory.

## 1. Cliff / Terrace Generation

**Current state**: Terrain heights are smooth Perlin noise (fBm). Mountains are smooth bumps — no cliffs, no sharp edges.

**Research questions**:
- How do games like Dwarf Fortress, Songs of Syx, and Minecraft generate cliff faces?
- What is the "terrace function" approach to height maps? (quantize heights into steps)
- How does thermal weathering erosion work? (loose material slides down steep slopes, creating talus at cliff bases)
- What erosion parameters create realistic cliff-to-valley transitions without thin spiky artifacts?
- How to detect and render cliff edges in a 2D top-down tile map?

**Practical target**: Mountains should have flat-topped plateaus with steep edges, valleys should have cliffside walls. Not every height change should be a cliff — only where the gradient exceeds some threshold.

## 2. Moisture-Based Vegetation

**Current state**: Vegetation is determined purely by elevation band (forest_level threshold). All grass at the same elevation looks the same regardless of water proximity.

**Research questions**:
- How do real biomes distribute based on moisture + temperature? (Whittaker biome diagram)
- How do games simulate moisture transport? (rain shadow effect from mountains, groundwater flow)
- What simple moisture diffusion algorithms work on a tile grid?
- How do rivers affect nearby vegetation density? (riparian zones)
- What's a good way to simulate grass → scrubland → forest gradients based on moisture levels?

**Practical target**: Dense forests near rivers, sparse grassland in dry areas, marsh/wetlands where water table is high. Farms near water should grow faster.

## 3. Soil Quality Model (Sand/Silt/Clay Triangle)

**Current state**: No soil model. Farm yield is constant everywhere. There's no concept of soil fertility.

**Research questions**:
- How does the USDA soil texture triangle (sand/silt/clay percentages) affect plant growth?
- How do real-world soil types correlate with terrain? (alluvial silt near rivers, sandy near coast, clay on plains, rocky near mountains)
- What simplified soil model would work for a game? (maybe 2-3 soil types with different farm multipliers)
- How does soil affect water drainage? (sandy = fast drain, clay = waterlogging)
- How do historical farming games (like Banished, Songs of Syx, Farming Simulator) model soil quality?

**Practical target**: Soil type derived from terrain during world gen. Farms on silt/river soil yield 2x, farms on rocky mountain soil yield 0.5x. Visible as a soil overlay.

## 4. Hydraulic Erosion Improvements

**Current state**: Basic hydraulic erosion with water flow downhill + height modification. Works but creates thin channels and flat-looking terrain.

**Research questions**:
- What is the "particle-based" hydraulic erosion algorithm? (drop individual water particles, track sediment)
- How does Sebastian Lague's erosion implementation work? (popular Unity tutorial)
- What parameters prevent thin 1-tile rivers while still creating natural valleys?
- How to create wide river plains vs narrow mountain streams?
- What is "sediment capacity" and how does flow speed affect erosion vs deposition?
- How to handle erosion at different scales (continental river systems vs local creek beds)?

**Practical target**: Rivers should be 2-5 tiles wide minimum. Valley floors should be flat-ish. Erosion should create visible drainage patterns, not just noise.

## 5. Groundwater / Water Table

**Current state**: Water exists on the surface only. No concept of groundwater, springs, or wells.

**Research questions**:
- How do games model water tables? (Dwarf Fortress has aquifers)
- What is a simple groundwater simulation? (water percolates down through soil, pools at impermeable layers)
- How do springs work? (groundwater emerging at surface where hillside meets water table)
- Could we use the existing moisture map as a proxy for groundwater?
- How would wells work gameplay-wise? (dig to water table for reliable water source)

**Practical target**: Springs that feed streams. Water table visible in query mode. Wells as a building type. Farms near high water table grow faster.

## 6. River Generation

**Current state**: No rivers. Water pools in low spots from erosion but doesn't form connected river networks.

**Research questions**:
- How do procedural river algorithms work? (start from mountain, A* to ocean following height gradient)
- What is the "drainage basin" approach to river generation?
- How wide should rivers be at different points? (narrow mountain stream → wide river delta)
- How do games handle river tiles vs regular water tiles?
- How to connect erosion simulation with deliberate river paths?

**Practical target**: 1-3 rivers per map connecting mountains to ocean/lakes. Rivers are 2-3 tiles wide, navigable. Settlement near river gets water access bonus.

---

## How to Use This Document

Feed these research questions to a deep research tool (ChatGPT Deep Research, Perplexity, etc.) with the prompt below. The results should be practical and implementation-focused, not purely theoretical.
