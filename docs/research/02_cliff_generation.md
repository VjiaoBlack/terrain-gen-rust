# Cliff Generation (Terraces + Thermal Weathering)

Goal: Mountains with flat tops and steep edges, without stair-step artifacts everywhere.

## Algorithm

### Step A: Terrace Reshape (quantized plateaus with controlled ramps)

```pseudocode
function terrace(h, W):
    // h: normalized height [0,1], W: terrace band width
    k = floor(h / W)              // terrace index
    f = (h - k * W) / W           // [0,1) within terrace
    s = min(2 * f, 1.0)           // first half ramps, second half flat
    return (k + s) * W
```

Apply ONLY where masked (not the whole map):

```pseudocode
mask = smoothstep(elev_min, elev_max, h) * ridge_mask(x, y)
h_new = lerp(h, terrace(h, W), mask)
```

`ridge_mask` comes from ridged noise OR (high elevation + high local curvature).

### Step B: Thermal Erosion (talus relaxation)

```pseudocode
repeat N iterations:
    for each cell p:
        for each neighbor n in 4-neighborhood:
            d = H[p] - H[n]
            if d > T:
                move = c * (d - T)
                H[p] -= move
                H[n] += move
```

Optimization: distribute only to the LOWEST neighbor. Updating heightmap immediately (not accumulating deltas) stabilizes results.

### Step C: Slope Classification for Rendering

```pseudocode
dzdx = H[x+1, y] - H[x-1, y]
dzdy = H[x, y+1] - H[x, y-1]
S = sqrt(dzdx^2 + dzdy^2)

if S > cliff_slope:     tile = CliffRock
else if S > talus_slope: tile = Scree
else:                    tile = Soil/Grass (biome-dependent)
```

## Recommended Parameters for 256x256

| Parameter | Starting Value | Notes |
|-----------|---------------|-------|
| Terrace band width `W` | 0.04-0.10 (normalized) or 15-50m | Smaller = more steps, larger = bolder mesas |
| Mask `elev_min/elev_max` | Top 20-35% of heights | Prevents "terraced farmland" on plains |
| Talus threshold `T` | 4/N = ~0.0156 (normalized) | For N=256 |
| Transfer fraction `c` | 0.5 | Higher causes oscillation |
| Thermal iterations | 15-40 (subtle) to 60-120 (strong scree) | All cheap at 256x256 |
| Cliff slope threshold | Empirical: ~95th percentile of slope histogram | |

## Implementation Priority

1. Implement terrace reshape with elevation mask (skip ridge mask initially, just use elevation threshold)
2. Implement thermal erosion (4-neighbor, immediate update)
3. Add slope classification for tile types
4. Later: refine ridge mask using curvature or ridged noise

## Key Pitfalls

- **Thin spiky artifacts after terracing**: Sharp quantization creates isolated high pixels. Fix: always run thermal erosion AFTER terracing. Optionally blur the terrace mask (not the heights).
- **Terraces everywhere (rice paddies look)**: Mask terracing to high elevation and ridge areas only. Never terrace the whole map.
- **Thermal erosion melts cliffs into gentle slopes**: If T is too low or iterations too high, everything relaxes. Fix: fewer iterations, higher T, or apply thermal erosion only where slope is already high (slope-gated talus).
- **c > 0.5 causes oscillation**: Keep transfer fraction at or below 0.5.
