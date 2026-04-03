# Documentation Index

Orientation guide for humans and agents working on terrain-gen-rust.

## Start Here

1. **[CLAUDE.md](CLAUDE.md)** — Build commands, module structure, key conventions, controls. Read this first for any code work.
2. **[docs/game_design.md](docs/game_design.md)** — Vision, design pillars (ranked), success criteria, phase roadmap, pillar deep-dives. **This is the north star.** Every feature and fix should trace back to a pillar.
3. **[docs/economy_design.md](docs/economy_design.md)** — Building costs, production chains, resource flow. Reference for balance work.

## Design Pillars (quick reference)

Ranked by priority. When two ideas conflict, the higher pillar wins.

1. **Geography Shapes Everything** — Terrain creates reasons, constraints, and asymmetry. Activity changes terrain over time. ([deep-dive](game_design.md#pillar-1-geography-shapes-everything))
2. **Emergent Complexity from Simple Agents** — Local knowledge, systems chain through simulation, simple rules create complex outcomes. Agent knowledge architecture: see/remember/share/truth layers. ([deep-dive](game_design.md#pillar-2-emergent-complexity-from-simple-agents))
3. **Explore / Expand / Exploit / Endure** — Natural game arc that emerges from simulation state. Gradient, not state machine. No phase gates. ([deep-dive](game_design.md#pillar-3-explore--expand--exploit--endure))
4. **Observable Simulation** — Two rendering modes: Map (symbolic ASCII) and Landscape (painterly terminal). If you can't see it, it doesn't count. ([deep-dive](game_design.md#pillar-4-observable-simulation))
5. **Scale Over Fidelity** — 500+ agents target. Spatial hash grid, path caching, tick budgets, hierarchical pathfinding. ([deep-dive](game_design.md#pillar-5-scale-over-fidelity))

## Key Architectural Insight

The **spatial hash grid** is the single highest-leverage infrastructure piece — it unlocks geography queries (P1), knowledge sharing (P2), per-tile rendering (P4), and AI/pathfinding performance (P5) simultaneously.

The **agent knowledge architecture** (P2) is the keystone system — it makes exploration meaningful (P3), geography matter (P1), simulation observable (P4), and forces scale solutions (P5).

## Anti-Goals

- No micromanagement (no individual villager control)
- No manual roads (emerge from traffic)
- No real-time combat controls (garrison placement is strategy)
- No dialogue or narrative text (simulation tells the story)
- No tech tree UI (building unlocks are implicit)
- No random resource spawning (resources exist at world-gen, discovered through exploration)

## Docs Reference

| Document | Purpose | When to read |
|----------|---------|-------------|
| [docs/game_design.md](docs/game_design.md) | Vision, pillars, phases, deep-dives | Before any feature work or design decision |
| [docs/economy_design.md](docs/economy_design.md) | Resource balance, building costs | When touching economy, buildings, production |
| [docs/agent_autoloop_review.md](docs/agent_autoloop_review.md) | Post-mortem on automated dev agent | Before setting up any automated agent work |
| [docs/playtest_notes.md](docs/playtest_notes.md) | Historical playtest data (15 games) | When investigating balance or regressions |
| [docs/terrain_research_topics.md](docs/terrain_research_topics.md) | Terrain algorithm research backlog | When working on terrain pipeline |
| [docs/research/](docs/research/) | Deep research on terrain algorithms | Reference for terrain pipeline implementation |

## Feature Design Docs (docs/design/)

41 detailed feature specs organized by pillar. Start with [INTEGRATION.md](docs/design/INTEGRATION.md) for the dependency graph and build order. See [KNOWN_CONFLICTS.md](docs/design/KNOWN_CONFLICTS.md) for unresolved design conflicts.

### Pillar 1: Geography Shapes Everything (12 docs)
| Doc | Summary |
|-----|---------|
| [precomputed_resource_map](docs/design/pillar1_geography/precomputed_resource_map.md) | Per-tile ResourceDeposit grid computed at world-gen |
| [geographic_resource_distribution](docs/design/pillar1_geography/geographic_resource_distribution.md) | Geological rules for resource placement by terrain |
| [terrain_driven_settlement](docs/design/pillar1_geography/terrain_driven_settlement.md) | Score-based building placement replacing radial scan |
| [mining_changes_terrain](docs/design/pillar1_geography/mining_changes_terrain.md) | Mountain->Quarry->QuarryDeep from mining activity |
| [farming_changes_terrain](docs/design/pillar1_geography/farming_changes_terrain.md) | Soil fertility, fallow recovery, visual exhaustion |
| [deforestation_regrowth](docs/design/pillar1_geography/deforestation_regrowth.md) | Forest->Stump->Bare->Sapling->Forest lifecycle |
| [rivers_as_barriers](docs/design/pillar1_geography/rivers_as_barriers.md) | Water impassable, fords, bridge building type |
| [water_proximity_farming](docs/design/pillar1_geography/water_proximity_farming.md) | Moisture-chain farm bonus near rivers |
| [seasonal_terrain_effects](docs/design/pillar1_geography/seasonal_terrain_effects.md) | Spring floods, winter ice, summer fire, autumn color |
| [chokepoint_detection](docs/design/pillar1_geography/chokepoint_detection.md) | Ray-casting corridor width for defensive positions |
| [elevation_advantage](docs/design/pillar1_geography/elevation_advantage.md) | Height grants sight, defense, uphill attack penalty |
| [forest_fire_spread](docs/design/pillar1_geography/forest_fire_spread.md) | CA fire with wind, ash fertility, firebreaks at rivers |

### Pillar 2: Emergent Complexity (9 docs)
| Doc | Summary |
|-----|---------|
| [per_villager_memory](docs/design/pillar2_emergence/per_villager_memory.md) | 32-entry ring buffer with confidence decay per villager |
| [stockpile_bulletin_board](docs/design/pillar2_emergence/stockpile_bulletin_board.md) | Knowledge sharing hub at stockpile visits |
| [local_awareness](docs/design/pillar2_emergence/local_awareness.md) | Replace global AI params with sight-range percepts |
| [simulation_chains](docs/design/pillar2_emergence/simulation_chains.md) | Effects chain through physics (water->moisture->crops) |
| [info_sharing_encounters](docs/design/pillar2_emergence/info_sharing_encounters.md) | Villagers exchange memories when nearby |
| [memory_decay](docs/design/pillar2_emergence/memory_decay.md) | Per-kind decay rates, stale arrival correction |
| [environmental_traces](docs/design/pillar2_emergence/environmental_traces.md) | Six pheromone-like scent layers on the world |
| [danger_memory](docs/design/pillar2_emergence/danger_memory.md) | Per-villager danger zones affecting pathfinding |
| [building_info_hubs](docs/design/pillar2_emergence/building_info_hubs.md) | Per-building-type specialized bulletin boards |

### Pillar 3: Explore/Expand/Exploit/Endure (4 docs)
| Doc | Summary |
|-----|---------|
| [threat_scaling](docs/design/pillar3_arc/threat_scaling.md) | Wealth-based threat score, geographic spawn directions |
| [seasonal_pressure_rhythm](docs/design/pillar3_arc/seasonal_pressure_rhythm.md) | Each season has distinct activity, risk, and feel |
| [outpost_mechanics](docs/design/pillar3_arc/outpost_mechanics.md) | Auto-triggered satellite settlements with supply lines |
| [milestone_notifications](docs/design/pillar3_arc/milestone_notifications.md) | 18 narrative milestones across the game arc |

### Pillar 4: Observable Simulation (7 docs)
| Doc | Summary |
|-----|---------|
| [map_rendering_mode](docs/design/pillar4_observable/map_rendering_mode.md) | Symbolic ASCII: flat color, behavior glyphs, no lighting |
| [landscape_rendering_mode](docs/design/pillar4_observable/landscape_rendering_mode.md) | Painterly terminal: color-dominant, lighting, weather |
| [entity_state_visibility](docs/design/pillar4_observable/entity_state_visibility.md) | Distinct glyph+color per BehaviorState |
| [activity_indicators](docs/design/pillar4_observable/activity_indicators.md) | Building particles: smoke, sparks, steam, dust |
| [resource_flow_visibility](docs/design/pillar4_observable/resource_flow_visibility.md) | Worn terrain from traffic, supply line overlay |
| [threat_visibility](docs/design/pillar4_observable/threat_visibility.md) | Wolf territory, approach corridors, garrison coverage |
| [dirty_rect_rendering](docs/design/pillar4_observable/dirty_rect_rendering.md) | Skip redrawing unchanged tiles (15x speedup) |

### Pillar 5: Scale Over Fidelity (7 docs)
| Doc | Summary |
|-----|---------|
| [spatial_hash_grid](docs/design/pillar5_scale/spatial_hash_grid.md) | 16x16 cell grid, O(nearby) lookups, 28x speedup |
| [path_caching](docs/design/pillar5_scale/path_caching.md) | Per-entity waypoint cache, invalidation rules |
| [tick_budgeting](docs/design/pillar5_scale/tick_budgeting.md) | Priority-based AI frequency (critical=1, idle=8 ticks) |
| [hierarchical_pathfinding](docs/design/pillar5_scale/hierarchical_pathfinding.md) | Two-level HPA* with 16x16 regions |
| [flow_fields](docs/design/pillar5_scale/flow_fields.md) | Reverse-Dijkstra for shared destinations |
| [group_flock_abstraction](docs/design/pillar5_scale/group_flock_abstraction.md) | Clustered same-activity villagers share AI evaluation |
| [data_oriented_arrays](docs/design/pillar5_scale/data_oriented_arrays.md) | SoA parallel arrays for cache-friendly AI hot path |

### Cross-Cutting (1 doc)
| Doc | Summary |
|-----|---------|
| [soil_degradation_system](docs/design/cross_cutting/soil_degradation_system.md) | Unified SoilFertilityMap shared by farming, mining, etc. |

## For Agents

If you are an AI agent working on this project:

1. Read `CLAUDE.md` for build/test commands and code conventions.
2. Read `docs/game_design.md` for design pillars — check your work against them.
3. Read `docs/design/INTEGRATION.md` for dependency graph and build order.
4. Read the specific feature design doc before implementing any feature.
5. Check `docs/design/KNOWN_CONFLICTS.md` for unresolved conflicts before starting work that touches conflicting areas.
6. Check `docs/agent_autoloop_review.md` for lessons from previous agent runs. Key takeaways:
   - Don't tweak thresholds without testing 10+ seeds.
   - Don't commit-revert-recommit. Test locally, commit once.
   - Use `--diagnostics` mode for structured telemetry, not screenshot parsing.
   - Separate diagnosis from treatment. Understand the system before changing it.
7. Anti-goals are hard constraints. Do not build features on the anti-goals list.
8. When in doubt between two approaches, the higher-ranked pillar wins.

## Current Status (2026-04-02)

- Terrain pipeline: 7 stages, 14 biomes, working
- Settlement sim: villagers, buildings, production chains, basic economy
- Diagnostics: `--diagnostics` flag emits JSONL telemetry
- Tests: 207 lib tests passing
- Design docs: 41 feature specs across 5 pillars + integration plan
- Next: resolve known conflicts, then build Phase 1 (spatial hash grid first)
