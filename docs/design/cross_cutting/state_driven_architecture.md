# State-Driven Simulation Architecture

**Status:** Adopted — all new systems should follow these principles.
**Applies to:** Terrain, hydrology, weather, moisture, vegetation, erosion, AI.

---

## Core Principle

> **State is truth. Systems create change. Derived data is a lens.**

---

## 1. Canonical State (Single Source of Truth)

All persistent, causal quantities live in one place. If something affects future evolution, it must be explicit state — not hidden inside a system or duplicated across modules.

**Canonical state fields:**
```
heights         — terrain elevation (modified by erosion)
water_depth     — surface water at each tile (ONE field, not three)
soil_moisture   — subsurface water content
humidity        — atmospheric moisture (currently wind.moisture_carried)
wind            — 2D velocity field
vegetation      — plant density per tile
discharge       — accumulated flow from hydrology erosion
momentum        — flow direction field (for meandering)
root_density    — vegetation's erosion resistance
```

**Rules:**
- If it accumulates over time → it's state
- If it affects future ticks → it must be explicit
- No duplication of the same concept across systems

---

## 2. Derived Data (Pure Functions of State)

Derived structures are computed FROM state. They are projections — never the source of truth.

**Examples of derived data:**
```
rivers        = where(water_depth > threshold)
biomes        = classify(heights, temperature, moisture, slope)
is_ocean      = heights < water_level
temperature   = f(heights, latitude, season)
walkability   = f(terrain_type, water_depth)
river_graph   = extract_flow_network(water_depth)
storm_cells   = detect(humidity, wind)
```

**Rules:**
- Pure functions (no side effects)
- Deterministic — same state → same output
- Recomputable at any time
- Can be cached with invalidation
- **NEVER treated as source of truth**

---

## 3. Systems (Only Writers to State)

All state mutations happen through systems. Systems read state + derived data, produce deltas, and those deltas are applied to produce the next state.

**Examples:**
```
rainfall_system(state, storms)    → Δsoil_moisture, Δwater_depth
evaporation_system(state)         → Δhumidity, Δwater_depth
flow_system(state)                → Δwater_depth
erosion_system(state)             → Δheights, Δdischarge
vegetation_system(state)          → Δvegetation, Δroot_density
```

**Rules:**
- Systems read state + derived data
- Systems produce deltas (ideally, not immediate mutation)
- Systems are the ONLY place that mutates state
- Derived data can influence HOW systems behave, but cannot directly mutate state

---

## 4. No Cycles Within a Tick

Derived data forms a DAG within a single tick:
```
state → rivers
state → storms
state → biomes
```

**Forbidden** (within same tick):
```
storms → humidity → storms
```

**Allowed** (through time):
```
state_t → storms → rainfall → humidity → state_t+1
```

Feedback loops operate BETWEEN ticks, not within them.

---

## 5. Simulation Loop (Phased)

Each tick has three phases. No interleaving.

```
Phase 1: Read State + Compute Derived
  rivers = extract(state)
  biomes = classify(state)

Phase 2: Compute Deltas
  Δrain = rainfall_system(state, derived)
  Δevap = evaporation_system(state)
  Δflow = flow_system(state)

Phase 3: Apply Updates
  state_t+1 = state_t + all deltas
```

---

## 6. No Dual Representations

Pick ONE canonical representation per concept. Everything else is a projection.

**Water example:**
- `water_depth` (grid) → **canonical**
- River graph → derived
- `Terrain::Water` → derived (should be `heights < water_level`, not stored)
- Discharge rendering → derived (view layer)

---

## 7. Views Are Projections

Multiple ways to observe state, all computed from state:

- **Human views:** terrain rendering, overlays, minimap
- **Agent views:** local patches, moisture zones, river graphs
- **Debug views:** height overlay, discharge overlay, delta maps

**Rule:** Views are `f(state)`. Never stored as state.

---

## Current Violations (What We Need to Fix)

| Principle | Current violation |
|---|---|
| Single Source of Truth | 3 water systems: Terrain::Water, pipe_water, discharge |
| No Dual Representations | Water is terrain enum AND depth field AND discharge value |
| Derived = Pure Functions | Biomes classified once at worldgen, never recomputed from live state |
| Views ≠ State | Rendering reads from 3 different water sources with different logic |
| Systems = Only Writers | No clear delta/commit pattern — systems mutate state directly |

---

## Design Heuristics

- If something is hard to keep in sync → it should be derived
- If something accumulates over time → it should be state
- If something affects future evolution → it must be explicit
- Prefer one-way data flow over bidirectional syncing
- Prefer time-based feedback loops over intra-tick cycles
