# Moisture and Biomes (Whittaker Diagram, Rain Shadow)

Goal: Wetness structure -- forests near rivers, marsh in low wet basins, dry rain shadows. Not just elevation rings.

## Algorithm

### Step A: Temperature Map

```pseudocode
for each cell (x, y):
    lat = y / (map_height - 1)                           // 0..1
    temp = lerp(temp_equator, temp_pole, abs(lat - 0.5) * 2)
    temp -= lapse_rate * max(0, height[x,y] - sea_level)
    temp += noise(x, y) * temp_noise_amp
```

### Step B: Rainfall with Orographic Precipitation + Rain Shadow

Fixed wind direction approximation:

```pseudocode
rain[all] = base_rain_noise(x, y)  // low-frequency Perlin, range [0.3, 1.0]

// March along wind direction (e.g., west-to-east)
for each row y:
    barrier = -INF
    for x = 0 to width-1:   // upwind to downwind
        barrier = max(barrier, height[x, y])

        // Orographic lift: more rain on uphill slopes
        if x > 0:
            rain[x, y] += orographic_gain * max(0, height[x,y] - height[x-1, y])

        // Rain shadow: barrier much higher than current cell reduces rain
        shadow = max(0, barrier - height[x, y] - shadow_clearance)
        rain[x, y] -= shadow_strength * shadow

        // Clamp rain to [0, max_rain]
        rain[x, y] = clamp(0, max_rain, rain[x, y])

        // Optional: decay barrier influence with distance
        barrier -= barrier_decay
```

### Step C: Surface Moisture from Water Proximity

```pseudocode
// Multi-source BFS from all water tiles (ocean, river, lake)
dist_to_water = BFS(water_tiles)  // integer tile distances
moisture_from_water[c] = exp(-dist_to_water[c] / d0)
```

### Step D: Combined Moisture

```pseudocode
moisture = w_rain  * normalize(rain)
         + w_water * normalize(moisture_from_water)
         - w_drain * normalize(drainage_proxy)
// drainage_proxy = slope (steeper drains faster) or soil permeability
```

### Step E: Biome Classification (Whittaker-style rectangles)

```pseudocode
function classify_biome(height, temp, moisture):
    if height < sea_level:              return Ocean
    if moisture > 0.75 and height near sea_level and slope < marsh_slope:
                                        return Marsh
    if temp < 0.15:                     return Tundra
    if temp < 0.25 and moisture < 0.4:  return Cold Desert
    if moisture < 0.20:                 return Desert
    if moisture < 0.35:                 return Scrubland
    if moisture < 0.50:                 return Grassland
    if moisture < 0.70:
        if temp > 0.6:                  return Savanna
        else:                           return Forest
    // moisture >= 0.70
    if temp > 0.7:                      return Tropical Rainforest
    else:                               return Temperate Rainforest / Wet Forest
```

Can also implement as a lookup table indexed by quantized (temp, moisture) bins.

## Recommended Parameters for 256x256

| Parameter | Starting Value | Notes |
|-----------|---------------|-------|
| `temp_equator` | 30.0 C | |
| `temp_pole` | -10.0 C | |
| `lapse_rate` | 6.5 C per 1000m | Standard atmospheric lapse rate |
| `temp_noise_amp` | 2-4 C | Local variation |
| `orographic_gain` | 0.1-0.3 | Keep small, bias not dominate |
| `shadow_strength` | 0.3-0.8 | |
| `shadow_clearance` | 0.05-0.15 (normalized height) | Height gap before shadow kicks in |
| `barrier_decay` | 0.002-0.01 per cell | Prevents permanent shadow |
| `d0` (water distance falloff) | 8-20 tiles | For meter-scale tiles |
| `w_rain, w_water, w_drain` | (0.5, 0.6, 0.3) | Tune so rivers create green corridors, shadows create dry basins |

## Implementation Priority

1. Temperature map (trivial, do first)
2. Basic rainfall from noise (no rain shadow yet)
3. Distance-to-water moisture (BFS, fast)
4. Whittaker biome classification with rectangles
5. Add rain shadow pass (orographic lift + shadow)
6. Tune weights to get visible biome variation

## Key Pitfalls

- **Biomes become blurry blobs**: Over-diffusing moisture. Use BFS distance (sharp local term), keep any iterative diffusion to 2-6 passes max.
- **Forests everywhere near coasts**: Ocean treated as equally wet as rivers. Fix: tag water types. Rivers/lakes can contribute stronger local moisture than ocean, or vice versa.
- **Rain shadows too strong / huge deserts with no transition**: Clamp the shadow reduction. Add distance decay to barrier. Only apply strong shadow behind barriers above a minimum height threshold.
- **Elevation rings (snowy peaks -> forest -> grass)**: This is what you get with elevation-only biomes. The moisture terms (rain shadow, distance-to-water) are specifically designed to break this pattern.
