# Hydraulic Erosion (Particle-Based and Grid-Based)

Goal: Realistic erosion that produces wide valleys (not 1-tile trenches), with controllable detail.

## Approach 1: Droplet/Particle Erosion (Sebastian Lague style)

### Core Algorithm

```pseudocode
for each droplet (total: num_droplets):
    pos = random_position()
    dir = (0, 0)
    speed = initial_speed        // 1.0
    water = initial_water        // 1.0
    sediment = 0

    for step in 0..max_lifetime:
        (height, gradient) = bilinear_height_and_gradient(pos)

        // Update direction: blend inertia with gradient
        dir = normalize(dir * inertia - gradient * (1 - inertia))
        pos += dir

        if out_of_bounds(pos) or dir == (0,0): break

        new_height = height_at(pos)
        delta_h = new_height - height

        // Sediment capacity depends on slope, speed, water
        capacity = max(-delta_h * speed * water * capacity_factor, min_capacity)

        if sediment > capacity or delta_h > 0:
            // DEPOSIT
            deposit = if delta_h > 0:
                          min(delta_h, sediment)
                      else:
                          (sediment - capacity) * deposit_speed
            sediment -= deposit
            add deposit to terrain (bilinear to 4 nearest grid points)
        else:
            // ERODE
            erode = min((capacity - sediment) * erode_speed, -delta_h)
            sediment += erode
            // Subtract erode from terrain using EROSION BRUSH RADIUS
            // This is the key width lever -- erodes a disk, not a point
            for each cell within erosion_radius of pos:
                weight = max(0, erosion_radius - distance) / sum_weights
                terrain[cell] -= erode * weight

        speed = sqrt(max(0, speed^2 + delta_h * gravity))
        water *= (1 - evaporate_speed)
```

### Why This Helps Width

The **erosion brush radius** naturally produces erosion over an area, suppressing 1-tile trenches. Radius 3-5 tiles creates visible valley floors.

## Approach 2: Grid-Based Hydraulic Erosion

```pseudocode
for each iteration:
    // 1. Add rainfall
    water[all] += rain_amount

    // 2. Compute flow using MFD (not D8!)
    for each cell c:
        for each downslope neighbor n:
            flow[c->n] = slope_weight(c, n) * water[c]

    // 3. Compute sediment capacity
    capacity[c] = K_c * slope[c] * water[c]

    // 4. Erode or deposit
    if sediment[c] < capacity[c]:
        erode = K_s * (capacity[c] - sediment[c])
        terrain[c] -= erode
        sediment[c] += erode
    else:
        deposit = K_d * (sediment[c] - capacity[c])
        terrain[c] += deposit
        sediment[c] -= deposit

    // 5. Transport water and sediment along flow directions
    // 6. Evaporate
    water[all] *= (1 - evaporation)
```

Grid-based produces recognizable erosion patterns and large flat valleys from deposition, but can over-flatten if run too long.

## Recommended Hybrid Strategy

1. Use MFD or D8 for flow accumulation and river identification (from hydrology step)
2. Carve/widen valleys explicitly from discharge (see hydrology doc)
3. Run a SMALL number of droplet iterations for local realism without eating mountains

## Recommended Parameters for 256x256

### Droplet Erosion

| Parameter | Default | Range | Notes |
|-----------|---------|-------|-------|
| `erosion_radius` | 3 | 2-8 | KEY width lever. 4-5 for 2-5 tile channels |
| `inertia` | 0.05 | 0.02-0.1 | Higher = smoother paths |
| `capacity_factor` | 4 | 2-8 | Sediment carrying capacity |
| `min_capacity` | 0.01 | | Prevents zero-capacity on flats |
| `erode_speed` | 0.3 | 0.2-0.4 | |
| `deposit_speed` | 0.3 | 0.2-0.4 | Keep balanced with erode |
| `evaporate_speed` | 0.01 | 0.005-0.05 | Too high = short gullies, too low = over-erode |
| `gravity` | 4 | 2-8 | |
| `max_lifetime` | 30 | 30-60 | Higher = longer rivers, more cost |
| `initial_water` | 1.0 | | |
| `initial_speed` | 1.0 | | |
| `num_droplets` | 10k-50k | | Cheap at 256x256 |

### Grid-Based

Balance rain, evaporation, and capacity so deposition matches incision. Otherwise everything becomes a flat sediment plain.

## Implementation Priority

1. Droplet erosion with erosion brush radius (biggest bang for effort)
2. Bilinear interpolation for height sampling and gradient computation
3. Tune erosion_radius for desired valley width
4. Later: grid-based erosion for broader landscape shaping
5. Later: explicit bank erosion step if droplets alone don't give enough width

## Key Pitfalls

- **Everything gets flat**: Too much deposition, too many iterations, or too aggressive talus smoothing. Fix: cap iteration counts. Run erosion only in an elevation band or near rivers. Re-inject mountainous detail with ridged noise AFTER large-scale carving but BEFORE final smoothing.
- **Persistent 1-tile channels**: D8 and single-cell erosion concentrate incision. Fix: use MFD flow for erosion. Use erosion brush radius >= 3. Add explicit bank erosion from discharge.
- **erodeSpeed >> depositSpeed causes over-incision**: Keep them balanced (both ~0.3). Monitor total terrain volume to detect runaway erosion.
- **Droplets cluster in valleys**: Random starting positions means droplets disproportionately flow to the same low points. This is somewhat realistic but can over-erode valleys. Fix: weight starting positions toward ridges/highlands, or cap per-cell erosion.
