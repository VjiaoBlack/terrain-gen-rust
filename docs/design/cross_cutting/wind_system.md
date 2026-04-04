# Feature: Wind System
Pillar: 1 (Geography), 2 (Emergence)
Priority: Core infrastructure
Phase: Next (post-Phase 5)

## What
A 2D wind vector field shaped by terrain that drives moisture transport, fire spread, and particle drift. Wind flows around mountains, funnels through passes, creates rain shadows on leeward sides.

## Why
Without wind:
- Moisture propagation is hardcoded +y (unrealistic)
- Vegetation surrounds water instead of following wind-carried moisture
- Fire has no directional spread
- Rain shadow doesn't exist at runtime (only baked at world-gen)
- The terrain doesn't shape weather — it's just backdrop

With wind, the terrain CREATES the weather. A mountain range blocks moisture → dry leeward side → different biome. A valley funnels wind → faster fire spread. Geography shapes everything (Pillar 1).

## Design

### Approach: Terrain-deflected wind + curl noise

NOT full fluid dynamics. Compute a static wind field from terrain, add noise for variation.

### Step 1: Base prevailing wind
Seasonal direction: westerly in spring/autumn, variable in summer, northerly in winter.
```rust
struct WindState {
    prevailing_direction: f64,  // radians, 0 = east, PI/2 = north
    prevailing_strength: f64,   // 0.0-1.0
    season_variation: f64,      // how much direction wobbles
}
```

### Step 2: Terrain deflection
For each tile, compute how the terrain deflects the base wind:
- Wind hits a slope → deflected perpendicular to the slope
- Mountain upstream → wind shadow (reduced speed on leeward side)
- Valley/pass → wind funnels, speed increases (conservation of mass)
- Flat terrain → wind passes through at base speed

Algorithm (compute once per wind direction change, ~5ms):
```
For each tile (x, y):
  1. March a ray FROM the tile UPWIND for 30 tiles
  2. Accumulate terrain height along the ray
  3. If terrain rises significantly: this tile is in wind shadow
     → reduce wind speed by shadow_factor
  4. Compute terrain gradient at this tile
  5. Deflect wind direction by gradient (dot product)
  6. If tile is in a narrow gap: boost speed (width ratio)
```

### Step 3: Curl noise turbulence
Add curl noise on top of the terrain-deflected field:
```rust
let turbulence_x = perlin.get([x * 0.05, y * 0.05, time * 0.01]);
let turbulence_y = perlin.get([x * 0.05 + 100.0, y * 0.05 + 100.0, time * 0.01]);
wind_x += turbulence_x * turbulence_strength;
wind_y += turbulence_y * turbulence_strength;
```
Curl noise is divergence-free (no artificial sources/sinks).

### Step 4: Moisture coupling
```
moisture_deposit(x, y) = wind_speed * wind_moisture * orographic_lift
```
- `wind_moisture`: decreases along wind path as moisture is deposited
- `orographic_lift`: when terrain forces air upward → cooling → precipitation
- Windward side of mountain: high moisture deposit (lush)
- Leeward side: low moisture (dry, rain shadow)

### Step 5: Fire coupling
Fire spread probability already exists as CA. Add wind modifier:
```
spread_prob *= 1.0 + 2.0 * dot(wind_direction, spread_direction)
```
Downwind spread is 3x more likely. Upwind spread is reduced.

### Step 6: Particle coupling
Smoke, embers, dust particles drift with local wind vector.
Already partially supported — particles have velocity.

## Data Structure
```rust
pub struct WindField {
    width: usize,
    height: usize,
    wind_x: Vec<f64>,  // per-tile x component
    wind_y: Vec<f64>,  // per-tile y component
    // Cached derived data
    wind_speed: Vec<f64>,
    wind_shadow: Vec<f64>,  // 0.0 = full shadow, 1.0 = no shadow
}
```
8 pipes (N/S/E/W/NE/NW/SE/SW) not needed — continuous vector field is smoother.

## Recomputation
- Full recompute when prevailing wind direction changes (seasonal, ~5ms)
- Curl noise updates every tick (cheap — just Perlin samples at entity positions)
- Wind shadow is static between direction changes

## Low pressure / wind shadow
Leeward side of mountains: wind speed drops, moisture delivery drops.
NOT modeled as actual pressure — just geometric attenuation from ray march.
Result: dry biomes form behind mountain ranges naturally.

## Integration Points
- `MoistureMap::update()` — wind carries moisture, deposits on windward slopes
- `check_fire_ignition()` / `tick_fire()` — wind direction affects spread
- Particle system — drift with wind
- `VegetationMap` — responds to moisture which responds to wind
- `reclassify_biomes()` — biomes shift as moisture patterns change with wind

## Edge Cases
- Wind direction change mid-fire: fire front adjusts gradually
- No wind (calm): all components fall back to current behavior
- Very strong wind: cap speed multiplier to prevent fire teleporting

## Test Criteria
- Windward side of mountain has higher moisture than leeward
- Fire spreads faster downwind than upwind
- Particles drift in wind direction
- Wind speeds up in narrow passes
- Wind direction changes seasonally

## Dependencies
- Existing: Perlin noise (terrain_gen.rs), MoistureMap, fire system, particle system
- None blocked — this is new infrastructure

## Estimated Scope
- WindState + prevailing direction: Small (1-2 hrs)
- Terrain deflection + ray march: Medium (4-6 hrs)
- Curl noise turbulence: Small (1-2 hrs)
- Moisture coupling: Medium (3-4 hrs)
- Fire + particle coupling: Small (2 hrs)
- Total: ~12-16 hours
