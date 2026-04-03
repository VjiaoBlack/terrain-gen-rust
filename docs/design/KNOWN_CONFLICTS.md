# Known Conflicts & Gaps in Design Docs

Identified during review of all 41 design docs. These must be resolved before implementation.

## Conflicts (two docs disagree)

### 1. Two competing resource map designs
- `pillar1_geography/precomputed_resource_map.md` — `Option<ResourceDeposit>` per tile, depletion in map
- `pillar1_geography/geographic_resource_distribution.md` — `ResourcePotential` u8 densities, depletion on entities
- **Resolution needed:** merge into one design. Likely: ResourcePotential for the raw geological data, ResourceDeposit for spawned minable entities.

### 2. Fire system duplication
- `pillar1_geography/forest_fire_spread.md` — detailed CA fire, `Burning` terrain (walkable, cost 10), WindState
- `pillar1_geography/seasonal_terrain_effects.md` — simpler fire_risk accumulator, `Fire` terrain (non-walkable)
- **Resolution needed:** use forest_fire_spread.md as the canonical fire system. seasonal_terrain_effects.md should reference it, not redefine it.

### 3. Scorched terrain defined twice
- Both fire docs define `Scorched` with potentially different properties.
- **Resolution:** one definition in forest_fire_spread.md, seasonal_terrain_effects.md references it.

### 4. Entity glyph table conflicts
- `pillar4_observable/entity_state_visibility.md` and `pillar4_observable/map_rendering_mode.md` define different glyphs per state
- **Resolution needed:** entity_state_visibility.md owns the glyph logic. map_rendering_mode.md references it. One canonical table.

### 5. Danger avoidance double-counted
- `pillar2_emergence/danger_memory.md` — per-villager A* cost multiplier (up to 6x)
- `pillar2_emergence/environmental_traces.md` — danger scent penalty (+2.0 cost)
- **Resolution needed:** define interaction rule. Likely: use max(memory, trace), not sum. Or: memory is personal avoidance, trace is collective signal, only one applies per tile.

### 6. Soil fertility: three homes
- `pillar1_geography/farming_changes_terrain.md` — `fertility: f64` on FarmPlot component
- `cross_cutting/soil_degradation_system.md` — unified SoilFertilityMap grid
- `pillar2_emergence/simulation_chains.md` — also proposes SoilFertilityMap
- **Resolution:** soil_degradation_system.md is canonical. FarmPlot reads from the grid, doesn't own fertility. farming_changes_terrain.md needs updating.

### 7. Flow fields designed twice
- `pillar5_scale/path_caching.md` Tier 3 sketches flow fields
- `pillar5_scale/flow_fields.md` is the full standalone design
- **Resolution:** flow_fields.md is canonical. path_caching.md Tier 3 should reference it.

### 8. AI function signature: two redesigns
- `pillar2_emergence/local_awareness.md` — NearbyEntities + StockpileFullness
- `pillar2_emergence/per_villager_memory.md` — VillagerMemory + BelievedStockpile
- **Resolution:** these are complementary (Layer 1 vs Layer 2). Need a merged proposal showing the final ai_villager signature with both.

## Gaps (nobody owns these)

### 1. Wind system
Referenced by: forest_fire_spread, activity_indicators, seasonal_terrain_effects.
**Needs:** standalone wind design doc or a section in seasonal_pressure_rhythm.md.

### 2. Terrain variant registry
12+ new terrain variants across 5 docs. No consolidated enum or base_terrain backup strategy.
**Needs:** a terrain_variants.md listing all variants, properties, and which doc introduces each.

### 3. system_assign_workers replacement
local_awareness.md Phase 3 and building_info_hubs.md both want to replace it. Neither defines the full replacement.
**Needs:** explicit design for the self-assignment transition.

### 4. Render mode dispatch
map_rendering_mode.md and landscape_rendering_mode.md both assume RenderMode enum. Neither owns it.
**Needs:** one doc to define the toggle, enum, and dispatch point in draw().

### 5. Save/load migration strategy
Many docs add serializable state. No unified migration plan.
**Needs:** save_migration.md defining versioned save format approach.

### 6. Bridge auto-build priority
rivers_as_barriers.md defines bridges but doesn't specify priority in auto_build_tick.
**Needs:** explicit priority ordering relative to existing 10+ building priorities.
