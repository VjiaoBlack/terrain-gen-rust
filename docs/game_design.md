# terrain-gen-rust — Game Design Document
*Last updated: 2026-04-02*

## Vision

You are watching a civilization discover and adapt to a landscape. The terrain isn't a backdrop — it IS the game. Every river creates a trade route, every mountain range creates a border, every mineral vein creates a reason to expand. The player doesn't control villagers — they shape the environment, and the village responds.

The core fantasy: **an ant colony meets geology.**

## Design Pillars (ranked)

When two ideas conflict, the higher pillar wins.

### 1. Geography Shapes Everything
Terrain is not decoration. Where the river runs determines where the village grows. Soil fertility determines which farms thrive. Mountain passes determine where threats arrive. Two different maps should produce two fundamentally different settlements — not the same cluster in a different color palette.

### 2. Emergent Complexity from Simple Agents
No scripted storylines. No hardcoded event chains. Villagers follow simple rules (eat, gather, sleep, flee) and interesting outcomes arise from system interactions. A drought + wolf raid + stone shortage should create a crisis organically, not because we wrote a "crisis event." The simulation is the story.

### 3. Explore → Expand → Exploit → Endure
The game has a natural arc. Early game: survive and discover what the land offers. Mid game: expand toward valuable resources, make strategic choices about where to grow. Late game: defend what you've built against escalating threats. The player's emotional journey should be: anxiety → ambition → pride → tension.

### 4. Observable Simulation
If you can't see it happening on screen, it doesn't count. Villagers carrying resources should be visible. Building construction should be visible. Threat approaching should be visible. The player's joy comes from watching systems interlock in real time, not reading a log panel.

### 5. Scale Over Fidelity
1000 simple agents beats 20 complex ones. Individual villagers don't need names, moods, or relationships. They need behavior that looks intelligent in aggregate. The camera should be able to zoom out and see the whole settlement as a living organism.

## Success Criteria

| Pillar | Success Looks Like | How to Measure |
|--------|-------------------|----------------|
| Geography Shapes Everything | Two seeds produce visibly different settlement shapes | Settlement footprint overlap <30% across 10 seeds |
| Emergent Complexity | Playtesters describe events the dev didn't plan | Unscripted 3+ step causal chains occur in >50% of games |
| Explore→Expand→Exploit→Endure | Each phase feels distinct when watching a 30K tick game | Diagnostics show phase transitions in resource/pop curves |
| Observable Simulation | A new player can narrate what's happening in <30s | Informal watch test: "what's that villager doing?" is answerable |
| Scale Over Fidelity | 500+ villagers at playable framerate | 60fps with pop >500 on release hardware |

## Current Phase: Foundation (Phase 1)

**Goal:** Make the terrain matter and the settlement feel alive.

Key deliverables:
- Precomputed resource map at world-gen (know where everything is from tick 0)
- Geography-driven settlement placement (near water, fertile soil, resources)
- Exploration as discovery (villagers reveal the map, find resource deposits)
- Threat escalation (wolves scale with settlement size, arrive from specific directions)
- Visible resource flow (gatherers walk to resources, carry them back, deposit visibly)

Done when: Playing seed 42 and seed 137 produces two settlements that look and feel different because of the terrain.

## Phase 2: Economy Depth

**Goal:** Make resource choices meaningful.

Key deliverables:
- Production chains that create real trade-offs (do I process wood→planks or save it for buildings?)
- Seasonal pressure (winter forces preparation, spring enables expansion)
- Scarcity-driven exploration (running low on stone → expand toward mountains)
- Building placement that matters (farms near water, garrison at chokepoints)

Done when: A player (or agent) has to make a real choice between two valid strategies at least 3 times per game.

## Phase 3: Scale

**Goal:** Go from 30 villagers to 500+.

Key deliverables:
- Performance at scale (spatial partitioning, LOD for distant agents)
- Emergent districts (residential, industrial, farming areas form naturally)
- Multi-settlement potential (outposts, resource colonies)
- Infrastructure (roads from traffic, bridges, aqueducts)

Done when: 500 villagers, 60fps, settlement looks like a living city from zoomed out.

## Phase 4: Threats & Mastery

**Goal:** Create reasons to care about what you've built.

Key deliverables:
- Escalating threats (bandit raids, wolf packs, natural disasters)
- Defensive geography (walls at chokepoints, garrisons at borders)
- Late-game mastery loop (optimizing production, defending efficiently)
- Win/loss conditions or endless sandbox with milestones

Done when: A settlement can be destroyed by neglect, and saving it feels earned.

## Anti-Goals

Things we are explicitly NOT building:
- **No micromanagement.** No assigning individual villagers to tasks. Ever.
- **No manual road placement.** Roads emerge from traffic patterns.
- **No real-time combat controls.** Garrison placement is the strategic layer; combat resolves automatically.
- **No dialogue or narrative text.** The simulation tells the story.
- **No tech tree UI.** Building unlocks are implicit (you can build a smithy when you have stone + a workshop).
- **No random resource spawning.** Resources exist in the world from generation. Villagers discover them through exploration.

## Open Questions

- Should villagers have any individual state beyond hunger/position/behavior? (Skills per-villager vs per-civilization?)
- How deep should water interaction go? (Irrigation? Fishing? River crossing penalties?)
- Should the player ever directly place anything other than buildings? (Designate mining areas? Set patrol routes?)
- Is there a z-level / elevation gameplay layer worth pursuing?
- Tower defense project — share a harness/engine, or keep separate and extract patterns later?
- What's the right balance between "realistic geology simulation" and "fun game with interesting terrain"?
