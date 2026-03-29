# Groundwater and Springs

Goal: Lightweight water table proxy for "forests near rivers," "marsh in low areas," farming yield variation, and spring placement -- without simulating 3D aquifers.

## Algorithm

### One scalar per tile: `groundwater` (stored water), derive `head` (water table elevation)

```pseudocode
head[c] = elevation[c] - bedrock_depth[c] + groundwater[c]
// Or simpler: head[c] = base_water_table + groundwater[c]
```

### Step A: Recharge (each worldgen pass or seasonal tick)

```pseudocode
for each cell c:
    recharge = rain[c] * permeability(soil[c])
    groundwater[c] += recharge
```

Permeability from soil type: Sand = high, Clay = low, Rocky = medium-high (fast drainage through cracks).

### Step B: Drain (baseflow + evapotranspiration)

```pseudocode
for each cell c:
    // Evapotranspiration
    groundwater[c] -= evap_rate * f(temp[c], biome[c])

    // Baseflow to rivers/ocean (if cell is river or ocean neighbor)
    if is_water_adjacent(c):
        groundwater[c] -= baseflow_rate * groundwater[c]
```

### Step C: Lateral Redistribution (lightweight diffusion)

```pseudocode
for i in 1..iters:
    for each cell c:
        for each neighbor n:
            flow = k * max(0, head[c] - head[n])
            // Optional: bias flow downhill with gradient_bias term
            groundwater[c] -= flow
            groundwater[n] += flow
```

### Step D: Spring Detection

```pseudocode
water_table_height[c] = some_base + groundwater[c]
// Or: head[c] as computed above

if water_table_height[c] >= elevation[c] + spring_eps:
    mark cell as spring
    // Optionally add as surface water source for river network
```

Springs form where the water table intersects the ground surface -- typically on hillsides or at the base of slopes with high recharge uphill.

### Step E: Impermeable Layers (simple hook, optional)

```pseudocode
// Per-tile bedrock_depth from noise
bedrock_depth[c] = noise(x, y) * max_bedrock_depth

// Shallow bedrock limits storage, increases lateral runoff
max_storage[c] = bedrock_depth[c] * porosity(soil[c])
groundwater[c] = min(groundwater[c], max_storage[c])
// Excess becomes surface runoff or springs
```

## Recommended Parameters for 256x256

| Parameter | Starting Value | Notes |
|-----------|---------------|-------|
| Lateral `k` | 0.02-0.08 of head difference per iteration | Smooth but not instantly flat |
| Diffusion iterations | 4-12 per seasonal tick; 30-100 at worldgen | Cheap at 65k cells |
| `spring_eps` | 0.01 * height_scale | Avoid marking every valley as spring |
| `evap_rate` | Scale with temperature and biome | Higher in desert/hot, lower in tundra |
| `baseflow_rate` | 0.1-0.3 per tick | Prevents infinite accumulation |
| `permeability` (Sand) | 0.8-1.0 | |
| `permeability` (Loam) | 0.4-0.6 | |
| `permeability` (Clay) | 0.1-0.2 | |
| `permeability` (Rocky) | 0.5-0.7 | Cracks allow drainage |

## Implementation Priority

1. Recharge from rainfall * soil permeability (needs soil model)
2. Lateral diffusion (simple loop, few iterations)
3. Spring detection (threshold check)
4. Baseflow drain to rivers/ocean (prevents runaway accumulation)
5. Later: impermeable layers / bedrock depth for perched aquifers
6. Later: seasonal tick updates for dynamic water table

## Key Pitfalls

- **Swamps everywhere**: Groundwater never drains. Fix: enforce baseflow sinks at ocean and river cells, and evapotranspiration. Water table should fluctuate with recharge and discharge.
- **No springs at all**: Head never reaches surface. Fix: increase recharge in high-rain zones, reduce permeability to trap water, explicitly check hillside intersection conditions.
- **Water table instantly flat**: k too high or too many iterations. Keep k small and iterations limited so water table follows terrain shape with some smoothing.
- **Aquifer as infinite water source** (Dwarf Fortress style): Fine for gameplay, but parameterize recharge floor so it can be tuned. DF treats aquifer tiles as infinite sources.
