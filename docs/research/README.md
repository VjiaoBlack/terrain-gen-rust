# Terrain Generation Research Index

Research notes extracted from deep-research report on practical terrain generation
for a tile-based Rust settlement sim targeting a 256x256 grid.

## Documents

1. [Pipeline Overview](01_pipeline_overview.md) -- Generation pipeline stages and scaling strategy
2. [Cliff Generation](02_cliff_generation.md) -- Terraces + thermal weathering for plateaus and scree
3. [Hydrology](03_hydrology.md) -- Depression filling, flow accumulation, river extraction and widening
4. [Moisture and Biomes](04_moisture_biomes.md) -- Whittaker diagram, rain shadow, moisture diffusion
5. [Soil Model](05_soil_model.md) -- USDA-inspired texture basis with 5 soil types
6. [Groundwater](06_groundwater.md) -- Lightweight water table proxy, springs, aquifers
7. [Hydraulic Erosion](07_hydraulic_erosion.md) -- Particle-based and grid-based erosion with valley widening

## Implementation Order

| Priority | Module | Depends On |
|----------|--------|------------|
| 1 | Pipeline / base height (fBm already exists) | -- |
| 2 | Cliff generation (terraces + thermal) | base height |
| 3 | Hydrology (depression fill, flow accum, rivers) | base height |
| 4 | Hydraulic erosion (droplet pass) | hydrology |
| 5 | Moisture and biomes | hydrology, temperature |
| 6 | Soil model | hydrology, biomes, slope |
| 7 | Groundwater | soil, hydrology |
