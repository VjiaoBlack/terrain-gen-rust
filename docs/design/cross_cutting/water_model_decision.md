# Design Decision: Water & Moisture Model

## The Problem (from diagnostics)

Two competing moisture transport systems that don't communicate:
1. MoistureMap (tile-level, slow 1-tile/tick diffusion, feeds vegetation)
2. WindField.moisture_carried (atmospheric, fast 3-tile/tick, feeds pipe_water)

Wind deposits rain into pipe_water, but pipe_water depths on land are too tiny to trigger tile moisture boost. Vegetation never sees wind-transported moisture.

Additionally, ocean pipe_water drains to near-zero because evaporation removes water with no replenishment.

## The Decision

### ONE moisture transport system

Remove the dual system. The flow should be:

```
Ocean (constant depth) 
  → evaporation (wind picks up moisture proportional to wind speed)
    → atmospheric moisture_carried (advected by wind, fast)
      → precipitation (orographic + saturation + cooling)
        → tile moisture (what vegetation reads)
          → also feeds pipe_water for surface flow
            → rivers/runoff flow back to ocean (pipe_water flows downhill)
```

**Key change:** Wind-deposited precipitation should DIRECTLY increase tile moisture, not go through pipe_water as intermediary. Pipe_water is for SURFACE water (visible rivers/lakes), tile moisture is for SOIL moisture (drives vegetation).

### Ocean = boundary condition

Ocean tiles (Terrain::Water) are a constant boundary:
- `pipe_water.get_depth(ocean_tile)` = always `water_level - terrain_height` (never changes)
- After each pipe_water.step(), reset ocean depths to their initial value
- Evaporation reads from ocean as infinite source (doesn't subtract from pipe_water)
- Rivers flowing into ocean are absorbed (pipe_water naturally drains to lowest point)

This means: total atmospheric + land water fluctuates, but ocean is the infinite reservoir that balances it.

### Vegetation growth scales with moisture

Currently binary: above 0.1 threshold = grow, below = decay. Should be:
- growth_rate = base_rate * moisture_level (proportional, not binary)
- High moisture (0.8) = fast growth
- Low moisture (0.2) = very slow growth  
- Below 0.05 = decay

### Remove box_blur from moisture

Box blur kills gradients and causes wrapping artifacts. Wind advection already provides spatial spread. If we need smoothing, use a small diffusion term, not a full blur.

## Implementation Plan (following new dev rules)

1. Write diagnostic test: "after 200 ticks, coastal tiles should have higher moisture than inland tiles by at least 2x"
2. Make ocean tiles constant in pipe_water (reset after each step)
3. Wind precipitation → tile moisture directly (not through pipe_water)
4. Remove MoistureMap step 2 (the slow advection — wind handles transport)
5. Remove box_blur from moisture update
6. Make vegetation growth proportional to moisture
7. Verify diagnostic test passes
8. Check biome distribution hasn't regressed

Each step verified independently.
