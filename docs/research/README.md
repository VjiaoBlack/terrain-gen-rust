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

## Additional Research
8. [Wind System](wind_system.md) — wind simulation and influence
9. [Erosion Systems](erosion_systems.md) — comparative erosion approaches
10. [Water Simulation](water_simulation.md) — water cycle and flow
11. [Analytical Erosion](analytical_erosion.md) — Stream Power Law approach
12. [Nick McDonald's Meandering](nickmcd_meandering.md) — river meandering algorithms
13. [Terrain Community](terrain_community.md) — community techniques and tools
14. [SoilMachine Deep Dive](soilmachine_deep_dive.md) — LayerMap, sediment graph, momentum

## Harness & Process Research
- [Self-Improving Harness](self_improving_harness.md) — agent harness iteration patterns
- [Game Dev Harness](game_dev_harness.md) — harness patterns specific to game development

## Cross-Referencing Convention

Each research doc should include:
- **Sources** section with URLs and access dates
- **See Also** section linking related research docs (use relative links)
- **Status** line: `Current` | `Stale (reason)` | `Superseded by X`

Periodic lint: check that docs match current code reality. Flag stale claims.
See also: `~/.claude/wiki/` for Victor's cross-project knowledge base.

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
