# Unified Hydrology System

**Status:** Adopted — supersedes `water_model_decision.md`
**Pillars:** Geography Shapes Everything (#1), Emergent Complexity (#2)
**Phase:** 1 (Core Infrastructure)
**Last updated:** 2026-04-01

---

## The Single Path

```
Ocean (constant boundary)
  → evaporation into wind.moisture_carried
    → advection by WindField (Stam solver)
      → precipitation: orographic + background
        → tile soil moisture (direct write)   ← vegetation reads this
        → pipe_water surface depth (saturation overflow only)
          → flows downhill, drains to ocean
```

`WindField.moisture_carried` is the only atmospheric moisture store. `MoistureMap.moisture` is the only soil moisture store. `PipeWater.depth` is only for visible surface water. Precipitation writes to both soil moisture and pipe_water in one pass — not sequentially through an intermediary.

---

## Diagnosed Failures Fixed

| Bug | Root Cause | Fix |
|-----|-----------|-----|
| Wind moisture never reaches vegetation | `moisture_carried` → `pipe_water`; depths too small to trigger `MoistureMap` boost | Precipitation writes directly to `MoistureMap.moisture` |
| Ocean pipe_water drains to zero | Evaporation removes depth; no replenishment | Ocean tiles are constant boundary, reset after each pipe step |
| Vegetation growth binary | Threshold `m > 0.1` → grow/decay with no rate scaling | Replace with `growth_rate = base_rate * moisture_factor(m)` |
| Box blur kills gradients | `box_blur()` flattens spatial variation every tick | Delete; wind advection is the only transport |
| Dual moisture systems | `MoistureMap` slow-diffuses independently of `WindField` | Delete slow-diffusion step; wind handles all transport |

---

## Data Structures

No new structs. Modifications to existing types only.

**`WindField`** (`src/simulation/wind.rs`) — struct unchanged. Add `update_moisture_carried()` fn: over ocean tiles load moisture proportional to wind speed (`evap_rate = 0.005`); over land, small evapotranspiration from soil moisture (`evapo_rate = 0.001`).

**`MoistureMap`** (`src/simulation/moisture.rs`) — remove `box_blur()` and the `delta[]` advection buffer. Keep `moisture`, `avg_moisture`, and EMA blend. Add passive decay `moisture[i] *= 0.995` per tick so un-rained tiles dry out.

**`PipeWater`** (`src/pipe_water.rs`) — add `ocean_mask: Vec<bool>` and `ocean_depth: Vec<f64>`, computed once from terrain at construction. After each `step()`, reset: `if ocean_mask[i] { depth[i] = ocean_depth[i]; }`.

**`VegetationMap`** (`src/simulation/vegetation.rs`) — add `grow_by(x, y, rate)` and `decay_by(x, y, rate)` so growth scales continuously with moisture.

---

## Implementation Steps

Each step has its own test. Do not proceed until the step's test passes.

**Step 1 — Ocean boundary condition** (`src/pipe_water.rs`).
Initialize `ocean_mask`/`ocean_depth` at construction. Reset ocean depths after `step()`.
_Test:_ After 500 ticks with rain, ocean tile depths remain within 1% of initial value.

**Step 2 — Wind evaporation from ocean** (`src/simulation/wind.rs`).
Add `update_moisture_carried(ocean_mask, soil_moisture, wind_speed)`. Ocean tiles load moisture each tick; land tiles evapotranspire at a lower rate.
_Test:_ After 200 ticks, coastal downwind tiles have `moisture_carried > 0.1`; value falls with distance inland.

**Step 3 — Precipitation writes directly to soil moisture** (`src/simulation/moisture.rs`).
Replace MoistureMap Steps 1+2+3 with a single pass: compute `orographic_lift = dot(wind, terrain_gradient).max(0)`, then `total_precip = moisture_carried[i] * (BACKGROUND_RATE + orographic_lift * OROGRAPHIC_RATE)`. Write `total_precip` directly to `self.moisture[i]`. Subtract from `wind.moisture_carried[i]`. When `moisture[i] >= SATURATION_THRESHOLD`, overflow fraction goes to `pipe_water.add_water()`.
Constants: `BACKGROUND_RATE = 0.002`, `OROGRAPHIC_RATE = 0.3`, `SATURATION_THRESHOLD = 0.8`.
_Test:_ After 100 ticks, tiles immediately downwind of ocean have soil moisture > 0.2. Tiles in mountain rain shadow have < half the moisture of windward tiles.

**Step 4 — Remove box blur and slow-diffusion** (`src/simulation/moisture.rs`).
Delete `box_blur()` and its call. Delete the `delta[]` advection buffer and wrapping-index loop. Add the passive decay term.
_Test:_ Uniform moisture seeded at 0.5 with no rain decays below 0.1 within 400 ticks. Spatial gradients from Step 3 persist without smearing.

**Step 5 — Proportional vegetation growth** (`src/simulation/moisture.rs`, `vegetation.rs`).
Replace the binary threshold with a continuous factor function: below 0.05 → slow decay; 0.05–0.15 → stasis; 0.15–0.85 → linear scale 0→1; above 0.85 → capped at 60% (waterlogged). Call `grow_by(factor)` / `decay_by(factor)` rather than the boolean grow/decay.
_Test:_ At `m = 0.5`, vegetation reaches 0.9 density within 1000 ticks from zero. At `m = 0.06`, vegetation decays from 0.9 to below 0.1 within 2000 ticks.

**Step 6 — Integration test** (`tests/hydrology_integration.rs`).
64x64 map, ocean on west edge, mountain ridge at x=30–35, 500 ticks. Assert: ocean depths constant (Step 1), west-of-ridge `avg_moisture > 0.35`, east-of-ridge `avg_moisture < west/2`, vegetation density west > east.

---

## What Does Not Change

The `WindField` Stam solver, `PipeWater` pipe-flow and sediment, `MoistureMap.avg_moisture` EMA, biome classification thresholds, and the terrain generation pipeline (stages 1–7) are all untouched.

---

## Related Docs

- `atmosphere_hydrology.md` — long-term wind+erosion loop (still valid post Step 6)
- `docs/research/nickmcd_meandering.md` — momentum map for rivers (future)
- `docs/research/analytical_erosion.md` — SPL closed-form erosion (future)
