# Soil Model (USDA-Inspired Texture Basis)

Goal: 3-5 soil types with gameplay-relevant properties (fertility, drainage, water retention), derived from terrain context.

## Algorithm

### Step A: Define Soil Types

| Soil Type | Drainage | Water Retention | Base Fertility | Yield Multiplier |
|-----------|----------|-----------------|----------------|-----------------|
| Sand | High | Low | Low | 0.6-0.8 |
| Loam (generic) | Medium | Medium | Medium | 1.0 |
| Alluvial Loam | Medium | High | High | 1.15-1.35 |
| Clay | Low | High | Medium (waterlog risk) | 0.9-1.1 |
| Rocky / Thin | Very high | Very low | Very low | 0.3-0.6 |
| Peat / Marsh | None (saturated) | Saturated | Low (unless drained) | 0.5 |

### Step B: Compute Helper Fields

```pseudocode
slope[c]           = magnitude of height gradient at c (central differences)
dist_to_coast[c]   = BFS distance from ocean tiles
river_influence[c] = BFS distance from river tiles, weighted by river discharge
                     // closer + bigger river = higher influence
```

### Step C: Assign Soil Type (priority order)

```pseudocode
function assign_soil(cell):
    if height[cell] < sea_level:
        return None  // water

    if wetness[cell] > marsh_wet and slope[cell] < marsh_slope:
        return Peat

    if slope[cell] > rocky_slope:
        return Rocky

    if dist_to_coast[cell] < coast_band and height[cell] < sea_level + beach_height:
        return Sand

    if river_influence[cell] > flood_thresh and slope[cell] < flood_slope:
        return AlluvialLoam

    if slope[cell] < clay_slope and height[cell] in lowland_band:
        return Clay

    return Loam  // generic default
```

### Step D: Compute Gameplay Values

```pseudocode
fertility = soil.fertility_base + bonus_from_moisture
drainage  = soil.drainage
water_cap = soil.water_holding
yield     = base_crop * fertility * f(temp) * f(season) * f(water_availability)
```

Alluvial fertility is justified by real floodplain mechanics: floods deposit nutrient-rich silt across flat areas.

## Recommended Parameters for 256x256

| Parameter | Starting Value | Notes |
|-----------|---------------|-------|
| `coast_band` | 2-6 tiles | Sand zone width |
| `rocky_slope` | ~80th-90th percentile of slope | Only steepest 10-20% of land |
| `flood_thresh` | Tune for ~2-8 tile alluvial corridor on main rivers | Scale with river size |
| `flood_slope` | Low (alluvium only on FLAT floodplains) | |
| `marsh_wet` | ~85th percentile of wetness | |
| `marsh_slope` | Very low (near flat) | |
| `lowland_band` | Below median elevation, above sea level | |
| `clay_slope` | Low-moderate | |

## Implementation Priority

1. Slope computation (reuse from cliff generation)
2. BFS distance fields (dist_to_coast, river_influence) -- reuse from hydrology/moisture
3. Soil assignment rules (the priority-order function above)
4. Attach gameplay constants per soil type
5. Later: break plain monotony with low-frequency noise for loam vs clay patches

## Key Pitfalls

- **Alluvial soil = overpowered "river = free food"**: If every river tile gives wide fertility, players always settle on rivers. Fix: scale alluvial width by river discharge AND require low slope (steep valleys have narrow floodplains).
- **Clay everywhere on plains**: Makes farming too uniform. Fix: use low-frequency noise to break plains into loam vs clay patches, but still bias clay to low, flat, poorly drained areas.
- **Rocky soil too rare or too common**: Tune the rocky_slope threshold by histogramming slopes and picking an appropriate percentile.
