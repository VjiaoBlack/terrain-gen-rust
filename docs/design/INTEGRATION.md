# Integration Plan: terrain-gen-rust Design Docs

*Master plan covering all 39 design docs in `docs/design/` plus `docs/game_design.md`.*
*Generated: 2026-04-01*

---

## 1. Dependency Graph (Critical Path DAG)

The DAG below shows which features must be built before others. Arrows mean "A must exist before B can ship." Features at the same depth can be built in parallel.

```
LAYER 0 — Shared Infrastructure (no dependencies)
  spatial_hash_grid
  per_villager_memory
  path_caching (Tier 1)
  precomputed_resource_map / geographic_resource_distribution
  terrain_driven_settlement (TerrainAnalysis struct)

LAYER 1 — Core Gameplay (depends on Layer 0)
  │
  ├─ local_awareness ──────────────────┐
  │    (depends on: spatial_hash_grid) │
  │                                    │
  ├─ stockpile_bulletin_board ─────────┤
  │    (depends on: per_villager_memory)│
  │                                    │
  ├─ deforestation_regrowth            │
  │    (no deps beyond tilemap)        │
  │                                    │
  ├─ mining_changes_terrain            │
  │    (no deps beyond tilemap)        │
  │                                    │
  ├─ simulation_chains ────────────────┤
  │    (SoilFertilityMap, moisture->farm wiring)
  │                                    │
  ├─ rivers_as_barriers ───────────────┤
  │    (Ford/Bridge terrain, A* changes)│
  │                                    │
  ├─ chokepoint_detection ─────────────┤
  │    (depends on: terrain_driven_settlement)
  │                                    │
  ├─ tick_budgeting ───────────────────┤
  │    (depends on: spatial_hash_grid) │
  │                                    │
  └─ threat_scaling ───────────────────┘
       (depends on: terrain_driven_settlement, chokepoint_detection)

LAYER 2 — Enrichment (depends on Layer 1 features)
  │
  ├─ memory_decay
  │    (depends on: per_villager_memory)
  │
  ├─ danger_memory
  │    (depends on: per_villager_memory, memory_decay)
  │
  ├─ info_sharing_encounters
  │    (depends on: per_villager_memory, stockpile_bulletin_board, spatial_hash_grid)
  │
  ├─ building_info_hubs
  │    (depends on: stockpile_bulletin_board, per_villager_memory)
  │
  ├─ environmental_traces
  │    (depends on: TrafficMap [exists], per_villager_memory)
  │
  ├─ farming_changes_terrain
  │    (depends on: simulation_chains [SoilFertilityMap], deforestation_regrowth-like patterns)
  │
  ├─ water_proximity_farming
  │    (depends on: simulation_chains [moisture→farm wiring])
  │
  ├─ seasonal_terrain_effects
  │    (depends on: simulation_chains, rivers_as_barriers, deforestation_regrowth)
  │
  ├─ seasonal_pressure_rhythm
  │    (depends on: seasonal_terrain_effects, farming_changes_terrain)
  │
  ├─ forest_fire_spread
  │    (depends on: deforestation_regrowth, simulation_chains [moisture])
  │
  ├─ elevation_advantage
  │    (depends on: terrain_driven_settlement [heights at runtime])
  │
  ├─ entity_state_visibility
  │    (no hard deps; rendering only)
  │
  ├─ map_rendering_mode
  │    (no hard deps; rendering only)
  │
  ├─ landscape_rendering_mode
  │    (no hard deps; rendering only)
  │
  ├─ milestone_notifications
  │    (depends on: threat_scaling [decouple milestones from threat_level])
  │
  ├─ threat_visibility
  │    (depends on: threat_scaling, chokepoint_detection)
  │
  └─ activity_indicators
       (depends on: entity_state_visibility, landscape_rendering_mode)

LAYER 3 — Scale & Polish (depends on Layer 2 features)
  │
  ├─ flow_fields
  │    (depends on: path_caching, spatial_hash_grid)
  │
  ├─ hierarchical_pathfinding
  │    (depends on: path_caching, spatial_hash_grid)
  │
  ├─ data_oriented_arrays
  │    (depends on: spatial_hash_grid)
  │
  ├─ group_flock_abstraction
  │    (depends on: spatial_hash_grid, tick_budgeting, per_villager_memory)
  │
  ├─ dirty_rect_rendering
  │    (no hard deps; rendering optimization)
  │
  ├─ resource_flow_visibility
  │    (depends on: entity_state_visibility, TrafficMap)
  │
  └─ outpost_mechanics
       (depends on: precomputed_resource_map, terrain_driven_settlement,
        rivers_as_barriers, threat_scaling, per_villager_memory)
```

### Critical Path

The longest dependency chain that gates the most features:

```
spatial_hash_grid
  -> local_awareness
  -> per_villager_memory (can start in parallel)
    -> stockpile_bulletin_board
      -> info_sharing_encounters
      -> building_info_hubs
```

And separately:

```
terrain_driven_settlement (TerrainAnalysis)
  -> chokepoint_detection
    -> threat_scaling (geographic spawns)
      -> threat_visibility
```

**The two "force multiplier" features are `spatial_hash_grid` and `terrain_driven_settlement`.** Almost everything depends on one or both.

---

## 2. Implementation Phases

### Phase 1: Foundation Infrastructure (8 features)

*Goal: Build the shared systems that everything else depends on.*

| Feature | Doc | Est. Hours | Pillar |
|---------|-----|-----------|--------|
| Spatial Hash Grid | `spatial_hash_grid.md` | 12-16 | 5 |
| Per-Villager Memory | `per_villager_memory.md` | 16-20 | 2 |
| Path Caching (Tier 1) | `path_caching.md` | 10-14 | 5 |
| Precomputed Resource Map | `precomputed_resource_map.md` / `geographic_resource_distribution.md` | 15-18 | 1 |
| Terrain-Driven Settlement | `terrain_driven_settlement.md` | 16-18 | 1 |
| Local Awareness (sight-range only) | `local_awareness.md` | 16-20 | 2 |
| Tick Budgeting | `tick_budgeting.md` | 10-12 | 5 |
| Simulation Chains (SoilFertility, moisture->farms) | `simulation_chains.md` | 12-16 | 2 |

**Phase 1 total: ~107-134 hours (~3-4 weeks full-time)**

*Internal ordering:* `spatial_hash_grid` first (unblocks local_awareness, tick_budgeting). `per_villager_memory` can start in parallel. `terrain_driven_settlement` can start in parallel. `precomputed_resource_map` can start in parallel. `path_caching` can start in parallel. `local_awareness` after spatial_hash_grid. `simulation_chains` can start anytime.

*Done when:* Villagers make decisions based on local sight + personal memory. Resources are geographically placed. Buildings go in terrain-appropriate spots. A* runs once per journey instead of once per tick. AI runs at budgeted frequency.

---

### Phase 2: Living World (11 features)

*Goal: Terrain changes from activity. Seasons matter. Information flows through the world.*

| Feature | Doc | Est. Hours | Pillar |
|---------|-----|-----------|--------|
| Deforestation & Regrowth | `deforestation_regrowth.md` | 8-10 | 1 |
| Mining Changes Terrain | `mining_changes_terrain.md` | 8-10 | 1 |
| Farming Changes Terrain | `farming_changes_terrain.md` | 10-14 | 1 |
| Water Proximity Farming | `water_proximity_farming.md` | 3 | 1 |
| Rivers as Barriers | `rivers_as_barriers.md` | 14-18 | 1 |
| Chokepoint Detection | `chokepoint_detection.md` | 16 | 1 |
| Stockpile Bulletin Board | `stockpile_bulletin_board.md` | 12-16 | 2 |
| Memory Decay | `memory_decay.md` | 8-10 | 2 |
| Danger Memory | `danger_memory.md` | 8-10 | 2 |
| Seasonal Terrain Effects | `seasonal_terrain_effects.md` | 16-20 | 1,4 |
| Milestone Notifications | `milestone_notifications.md` | 6-8 | 3 |

**Phase 2 total: ~109-135 hours (~3-4 weeks full-time)**

*Internal ordering:* `deforestation_regrowth` + `mining_changes_terrain` + `rivers_as_barriers` can start immediately (terrain-only changes). `chokepoint_detection` after `terrain_driven_settlement`. `stockpile_bulletin_board` after `per_villager_memory`. `farming_changes_terrain` + `water_proximity_farming` after `simulation_chains`. `seasonal_terrain_effects` after `simulation_chains` + `rivers_as_barriers` + `deforestation_regrowth`. `memory_decay` + `danger_memory` after `per_villager_memory`.

*Done when:* Forests thin around the settlement. Mountains show quarry scars. Rivers block movement and create strategic crossings. Seasons are visible on the map. Villagers share knowledge at the stockpile and remember danger.

---

### Phase 3: Threats & Defense (5 features)

*Goal: The Endure phase emerges from geography-driven threats.*

| Feature | Doc | Est. Hours | Pillar |
|---------|-----|-----------|--------|
| Threat Scaling (wealth-based, geographic spawns) | `threat_scaling.md` | 20-24 | 1,3 |
| Elevation Advantage | `elevation_advantage.md` | 8-10 | 1 |
| Forest Fire Spread | `forest_fire_spread.md` | 16-20 | 1,2 |
| Seasonal Pressure Rhythm | `seasonal_pressure_rhythm.md` | 16-20 | 3 |
| Threat Visibility | `threat_visibility.md` | 14-18 | 4 |

**Phase 3 total: ~74-92 hours (~2-3 weeks full-time)**

*Internal ordering:* `threat_scaling` first (unblocks `threat_visibility`). `elevation_advantage` can be done anytime (reads `heights`). `forest_fire_spread` after `deforestation_regrowth` (Phase 2). `seasonal_pressure_rhythm` after `seasonal_terrain_effects` (Phase 2).

*Done when:* Wolves emerge from forests. Raiders approach through mountain passes. Hilltop garrisons see further. Fire sweeps through dry summer forests. Winter is genuinely dangerous.

---

### Phase 4: Observable Simulation (6 features)

*Goal: Everything that happens is visible. The terminal window becomes a porthole into a living world.*

| Feature | Doc | Est. Hours | Pillar |
|---------|-----|-----------|--------|
| Entity State Visibility | `entity_state_visibility.md` | 10-12 | 4 |
| Map Rendering Mode | `map_rendering_mode.md` | 10-14 | 4 |
| Landscape Rendering Mode | `landscape_rendering_mode.md` | 20-28 | 4 |
| Activity Indicators | `activity_indicators.md` | 8-10 | 4 |
| Resource Flow Visibility | `resource_flow_visibility.md` | 12-16 | 4 |
| Dirty-Rect Rendering | `dirty_rect_rendering.md` | 8-10 | 5 |

**Phase 4 total: ~68-90 hours (~2-3 weeks full-time)**

*Internal ordering:* `entity_state_visibility` first (unblocks `activity_indicators` and `resource_flow_visibility`). `map_rendering_mode` + `landscape_rendering_mode` can proceed in parallel. `dirty_rect_rendering` is independent.

*Done when:* A player can tell what every villager is doing at a glance. Two rendering modes work. The settlement breathes with smoke, sparks, and worn paths.

---

### Phase 5: Scale & Expansion (7 features)

*Goal: 500+ villagers at 60fps. Multi-settlement expansion.*

| Feature | Doc | Est. Hours | Pillar |
|---------|-----|-----------|--------|
| Flow Fields | `flow_fields.md` | 14-18 | 5 |
| Hierarchical Pathfinding | `hierarchical_pathfinding.md` | 20-24 | 5 |
| Data-Oriented Arrays | `data_oriented_arrays.md` | 12-16 | 5 |
| Group/Flock Abstraction | `group_flock_abstraction.md` | 16-20 | 5 |
| Info Sharing on Encounter | `info_sharing_encounters.md` | 10-12 | 2 |
| Building Info Hubs | `building_info_hubs.md` | 14-18 | 2 |
| Outpost Mechanics | `outpost_mechanics.md` | 30-34 | 3 |

**Phase 5 total: ~116-142 hours (~3-4 weeks full-time)**

*Internal ordering:* `flow_fields` + `hierarchical_pathfinding` can proceed in parallel (both depend on `path_caching` from Phase 1). `data_oriented_arrays` after `spatial_hash_grid`. `group_flock_abstraction` after `spatial_hash_grid` + `tick_budgeting`. `info_sharing_encounters` + `building_info_hubs` after `stockpile_bulletin_board` (Phase 2). `outpost_mechanics` is the capstone -- depends on resource map, terrain scoring, rivers, threats, and memory.

*Done when:* 500 villagers at 60fps. Outposts form at distant resources. Information flows through encounters. Villagers cluster into efficient groups.

---

## 3. Shared Infrastructure (Top 5 Force Multipliers)

These systems are used by the most downstream features. Build them first and build them well.

| # | System | Doc | Used By (count) | What It Enables |
|---|--------|-----|-----------------|-----------------|
| 1 | **Spatial Hash Grid** | `spatial_hash_grid.md` | 14 features | Every spatial query in AI, rendering, encounters, group detection, threat checks. Replaces O(N) scans with O(nearby). |
| 2 | **Per-Villager Memory** | `per_villager_memory.md` | 10 features | Foundation for all knowledge systems: bulletin board, encounter sharing, danger memory, memory decay, building hubs, outposts. Without it, villagers are omniscient. |
| 3 | **TerrainAnalysis struct** | `terrain_driven_settlement.md` | 7 features | `slope`, `dist_to_river`, `dist_to_water`, `chokepoint` scores. Used by building placement, chokepoint detection, threat scaling, elevation advantage, farm placement, auto-build. |
| 4 | **Precomputed Resource Map** | `precomputed_resource_map.md` | 6 features | Ground truth for resource locations. Used by exploration, settlement knowledge, outpost triggers, AI gather decisions, resource overlay. Replaces hardcoded spawns. |
| 5 | **Path Caching (Tier 1)** | `path_caching.md` | 5 features | Foundation for flow fields, hierarchical pathfinding, danger-aware routing. Eliminates per-tick A* recomputation. All movement systems build on it. |

---

## 4. Integration Risks

### A. Conflicting Terrain Variants

**Risk:** Four docs propose new `Terrain` enum variants totaling 12+ new types.

| Doc | New Variants |
|-----|-------------|
| `deforestation_regrowth.md` | `Stump`, `Bare`, `Sapling` |
| `mining_changes_terrain.md` | `Quarry`, `QuarryDeep`, `ScarredGround` |
| `forest_fire_spread.md` | `Burning`, `Scorched`, `AshGround` |
| `seasonal_terrain_effects.md` | `FloodWater`, `Ice`, `Fire`, `Scorched` |
| `rivers_as_barriers.md` | `Ford`, `Bridge` |

**Conflicts:**
- `forest_fire_spread.md` and `seasonal_terrain_effects.md` both define `Scorched` and `Fire` with slightly different properties (different glyphs, colors, A* costs). Must reconcile into a single definition.
- `seasonal_terrain_effects.md` proposes `base_terrain: Vec<Terrain>` for temporary seasonal overlays (Ice over Water, FloodWater over Grass). `forest_fire_spread.md` also needs temporary terrain (Burning -> Scorched -> AshGround). These should share a single `base_terrain` backup grid.

**Mitigation:** Define ALL new terrain variants in one commit. Create a shared `base_terrain` grid used by both seasonal and fire systems. Standardize `Scorched`/`Fire` properties across both docs.

### B. Two Resource Map Designs

**Risk:** Both `precomputed_resource_map.md` and `geographic_resource_distribution.md` design a resource map. They overlap heavily but differ in structure.

| Aspect | `precomputed_resource_map.md` | `geographic_resource_distribution.md` |
|--------|------|------|
| Per-tile data | `Option<ResourceDeposit>` (type + richness + remaining + quality) | `ResourcePotential` (stone/wood/fertility/food/iron/clay as u8 densities) |
| Entity spawning | Option B: no entities, villagers interact with map directly | Entities spawned from map at threshold |
| Depletion | Tracked in map (`remaining` field) | Tracked on entities (`ResourceYield`) |

**Conflict:** These are two approaches to the same problem. Shipping both creates confusion.

**Mitigation:** Merge into one design. Use `ResourcePotential` (u8 densities per resource type) as the pipeline output. Spawn entities for high-density deposits (threshold approach from `geographic_resource_distribution.md`). Track depletion on entities as today. The resource map is ground truth for AI scoring and overlay rendering; entities are the interactive layer.

### C. Per-Villager Memory vs Environmental Traces

**Risk:** `per_villager_memory.md` and `environmental_traces.md` both solve "how do villagers find resources they haven't seen?" with different mechanisms.

- Per-villager memory: personal, precise, decays per-kind.
- Environmental traces: communal, fuzzy, decays per-layer.

`danger_memory.md` adds per-villager danger avoidance. `environmental_traces.md` adds danger scent as a world-level trace. Both modify A* costs.

**Conflict:** If both are active, danger is double-counted in pathfinding (personal memory penalty + trace penalty).

**Mitigation:** Per-villager memory is Layer 2 (personal). Environmental traces are Layer 2.5 (ambient). They should stack but with diminishing returns. Danger from personal memory should dominate when present; trace scent is the fallback for villagers without firsthand experience. Cap total pathfinding penalty at a maximum multiplier (e.g., 8x).

### D. Fire System Duplication

**Risk:** `forest_fire_spread.md` and `seasonal_terrain_effects.md` both design fire systems with overlapping but different specs.

| Aspect | `forest_fire_spread.md` | `seasonal_terrain_effects.md` |
|--------|---------|---------|
| Ignition | Lightning + smithy adjacency, moisture < 0.15 | fire_risk accumulator, ignition at threshold 0.8 |
| Spread | Per-tick cellular automata with wind | Per-5-tick spread to adjacent flammable tiles |
| Terrain | `Burning` (walkable, cost 10) | `Fire` (non-walkable) |
| Wind | Full `WindState` system | Not specified |

**Conflict:** Two different fire implementations. The `forest_fire_spread.md` design is more detailed and complete.

**Mitigation:** Use `forest_fire_spread.md` as the canonical fire design. `seasonal_terrain_effects.md`'s Phase D (fire) should defer to it entirely. The `fire_risk` accumulator from `seasonal_terrain_effects.md` is a reasonable ignition gate and can be folded into `forest_fire_spread.md`'s ignition conditions.

### E. system_farms Signature Change

**Risk:** Three docs propose changes to `system_farms`: `simulation_chains.md` (add moisture + fertility), `water_proximity_farming.md` (add moisture), `farming_changes_terrain.md` (add fertility + soil_type). All three add `tile_x`, `tile_y` to `FarmPlot`.

**Conflict:** None -- these are additive and compatible. The risk is purely sequencing: whoever lands first defines the signature, others adapt.

**Mitigation:** `simulation_chains.md` goes first (Phase 1). It adds `tile_x/tile_y`, `&MoistureMap`, and `&SoilFertilityMap` to the signature. `water_proximity_farming.md` becomes a no-op for growth wiring (already done) and just adds fallow recovery bonus. `farming_changes_terrain.md` adds fertility degradation/fallow logic using the existing SoilFertilityMap.

### F. Rendering Mode Conflicts

**Risk:** `entity_state_visibility.md` and `map_rendering_mode.md` both define villager glyphs per behavior state, with slightly different tables.

| State | `entity_state_visibility.md` | `map_rendering_mode.md` |
|-------|--------|--------|
| Idle | `○` dim blue | `@` cyan |
| Gathering(Wood) | `♠` brown | `$` brown |
| Farming | `∞` or `~` green | `f` green |
| Exploring | `►` cyan | `?` light green |

**Conflict:** Two competing glyph vocabularies for the same system.

**Mitigation:** `map_rendering_mode.md` is the Map Mode spec. `entity_state_visibility.md` is the Landscape Mode spec. Each mode has its own glyph table. The `entity_visual()` function takes a `RenderMode` parameter and returns the appropriate glyph/color. This is already implied by both docs but should be made explicit.

---

## 5. Quick Wins

Features that are small scope, low dependency, and high visible impact. Good first tasks or tasks to slot between larger features.

| Feature | Doc | Est. Hours | Dependencies | Impact |
|---------|-----|-----------|--------------|--------|
| **Milestone Notifications** | `milestone_notifications.md` | 6-8 | None (refactor existing) | Immediate narrative improvement; player sees settlement story |
| **Entity State Visibility** | `entity_state_visibility.md` | 10-12 | None | Answers "what is that villager doing?" at a glance |
| **Deforestation & Regrowth** | `deforestation_regrowth.md` | 8-10 | None (new terrain variants) | Visible terrain change from activity; wood becomes finite |
| **Mining Changes Terrain** | `mining_changes_terrain.md` | 8-10 | None (new terrain variants) | Mountains show quarry scars; visible industrial history |
| **Water Proximity Farming** | `water_proximity_farming.md` | 3 | `simulation_chains` Step 2 | Rivers matter for farming with ~1.5 hours of code |
| **Memory Decay** | `memory_decay.md` | 8-10 | `per_villager_memory` | Visible wasted trips; scouts become valuable |
| **Activity Indicators** | `activity_indicators.md` | 8-10 | Particle system (exists) | Settlement breathes; smithy glows orange |
| **Dirty-Rect Rendering** | `dirty_rect_rendering.md` | 8-10 | None | 15x draw cost reduction; free FPS headroom |

---

## 6. Estimated Total Scope

| Phase | Features | Hours (est.) | Calendar (full-time) |
|-------|----------|-------------|---------------------|
| Phase 1: Foundation Infrastructure | 8 | 107-134 | 3-4 weeks |
| Phase 2: Living World | 11 | 109-135 | 3-4 weeks |
| Phase 3: Threats & Defense | 5 | 74-92 | 2-3 weeks |
| Phase 4: Observable Simulation | 6 | 68-90 | 2-3 weeks |
| Phase 5: Scale & Expansion | 7 | 116-142 | 3-4 weeks |
| **Total** | **37 features** | **474-593 hours** | **13-18 weeks full-time** |

**Notes on the estimate:**
- Two docs (`precomputed_resource_map.md` and `geographic_resource_distribution.md`) are merged into one feature (counted once).
- `environmental_traces.md` is not separately phased -- its foot traffic layer already exists, and other layers fold into Phase 2 (danger) and Phase 5 (gather scent, home scent).
- Hours include implementation + testing but not cross-feature integration testing. Add ~15% buffer for integration work.
- A realistic calendar for a single developer with other commitments: 6-9 months.
- Phases 4 and 5 can overlap since rendering work (Phase 4) is independent of scale optimization (Phase 5).

### Recommended Build Order (What to Build Next)

If starting from the current codebase today:

1. **spatial_hash_grid** -- unblocks the most features, pure infrastructure, no behavior change
2. **path_caching (Tier 1)** -- biggest single performance win, can ship independently
3. **per_villager_memory** -- unblocks the entire knowledge architecture
4. **deforestation_regrowth** + **mining_changes_terrain** -- quick wins, visible terrain impact
5. **entity_state_visibility** -- quick win, immediate readability improvement
6. **precomputed_resource_map** -- makes geography drive strategy
7. **terrain_driven_settlement** -- makes building placement intelligent
8. **local_awareness** -- the big behavior change that makes villages feel like ant colonies
