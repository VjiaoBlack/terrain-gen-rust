# Feature: Dynamic Terrain Classification
Pillar: 1 (Geography), 4 (Observable)
Priority: Core rework

## What
Terrain biome types should be derivable from current simulation state (soil, vegetation, moisture, temperature), not frozen at world-gen. The rendering should show what the land IS right now, not what it was born as.

## Why
Currently a Forest tile with zero vegetation (all trees cut) is still "Forest" — the biome enum is assigned once in the pipeline and never changes. This causes:
- Deforested areas still claim to be forest (AI seeks wood there)
- Runtime moisture changes don't affect biome (irrigated desert stays desert)
- Elevation dominates biome assignment because temperature and moisture correlate with height
- The rendering hack (soil+vegetation blend) partially hides this but the underlying data is wrong

## Current State

### Pipeline data (computed once, partially discarded)
| Data | Stored on Game? | Used at runtime? |
|------|-----------------|------------------|
| `heights: Vec<f64>` | Yes | Yes (elevation, pathfinding) |
| `temperature: Vec<f64>` | **No — discarded** | No |
| `slope: Vec<f64>` | **No — discarded** | No |
| `pipeline_moisture: Vec<f64>` | **No — discarded** | No |
| `soil: Vec<SoilType>` | Yes | Yes (farm yields, fertility) |
| `river_mask: Vec<bool>` | Yes | Yes (fords, floods) |
| `resources: ResourceMap` | Yes | Yes (entity spawning) |

### Runtime data (updated every tick)
| Data | Drives rendering? |
|------|------------------|
| `VegetationMap` | Yes (texture density in landscape, color blend) |
| `MoistureMap` | Yes (farm growth), No (not used for color) |
| `WaterMap` | Indirect (feeds moisture) |
| `SoilFertilityMap` | Yes (farm fallow color) |

### The gap
Pipeline moisture → biome classification is ONE-WAY and ONE-TIME.
Runtime MoistureMap is a separate system that doesn't feed back into biome.
VegetationMap is a separate system that doesn't feed back into biome.

## Design

### Step 1: Persist pipeline data on Game
Store `temperature`, `slope`, and `pipeline_moisture` on Game (they're already in PipelineResult, just not copied over). ~50 lines, zero behavior change.

### Step 2: Make rendering fully data-driven
Instead of `terrain.fg() / terrain.soil_fg()`, compute color from:
```
base_color = f(soil_type, height)           // grey for rock, brown for loam, tan for sand
green_tint = f(vegetation_level)             // 0.0 = no green, 1.0 = lush
moisture_tint = f(runtime_moisture)          // wet areas slightly darker/richer
temperature_tint = f(temperature)            // cold areas blue-shifted
final_color = blend(base_color, green_tint, moisture_tint, temperature_tint)
```
The Terrain ENUM still exists for walkability, A* costs, building rules. But COLOR is fully data-driven.

### Step 3: Dynamic biome reclassification (optional, bigger change)
Every N ticks (or on terrain change), reclassify tiles:
```
if vegetation > 0.6 && moisture > 0.4 → Forest (for AI purposes)
if vegetation < 0.1 && was Forest → Grass or Bare (deforested)
if moisture < 0.15 → Desert/Scrubland
```
This makes the biome enum DESCRIPTIVE (what the land is now) not PRESCRIPTIVE (what the pipeline said it should be).

### Step 4: Wind system
Replace the hardcoded +y moisture propagation with actual wind:
- `WindState { direction: f64, strength: f64 }` on Game
- Seasonal wind direction (prevailing westerlies, etc.)
- Moisture propagation follows wind direction
- Rain shadow effect becomes dynamic (was static in pipeline)

### Step 5: Initialize VegetationMap from biome
Currently VegetationMap starts at 0.0 everywhere and slowly grows. Instead, initialize from the pipeline biome classification:
- Forest tiles → vegetation 0.8
- Grass → 0.4
- Scrubland → 0.2
- Desert → 0.05
This makes the game START with visible vegetation instead of growing from nothing.

## Implementation Order
1. Persist pipeline data (quick, unblocks everything)
2. Initialize VegetationMap from biome (quick, big visual impact)
3. Data-driven color (medium, the big visual change)
4. Wind system (medium, needed for realistic moisture)
5. Dynamic reclassification (large, changes AI behavior)

## Edge Cases
- Performance: reclassifying 256x256 = 65K tiles is ~1ms, fine even every 100 ticks
- Save/load: pipeline data is deterministic from seed, can recompute on load
- Biome transition visuals: avoid "popping" — use smooth color blend not sudden type change

## Dependencies
- #57 soil/vegetation color split (done)
- Existing VegetationMap, MoistureMap, SoilFertilityMap

## Estimated Scope
- Steps 1-2: Small (1-2 hours)
- Step 3: Medium (4-6 hours)  
- Step 4: Medium (4-6 hours)
- Step 5: Large (8-12 hours)
