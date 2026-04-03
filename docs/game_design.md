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

---

## Pillar Deep-Dives

### Pillar 1: Geography Shapes Everything

**The problem today:** A settlement on seed 42 (grassland) and seed 137 (desert) play out the same way — build huts, build farms, gather wood, gather stone — just with different success rates. Terrain makes it *harder or easier* but doesn't change *what you do*.

**What "geography shapes everything" actually means:**

**A. Terrain creates REASONS to go somewhere.**
Resources aren't near spawn. Stone is in the mountains to the east. The river valley has alluvial soil. The forest belt to the north is the only lumber source. Expansion is motivated by what the land offers in a specific direction.

**B. Terrain creates CONSTRAINTS on how you grow.**
A river you can't easily cross → settlement grows along one bank until you bridge it. A mountain range → natural wall, but also blocks expansion. A lake → can't build there, but maybe fishing. Terrain doesn't just slow movement — it shapes the settlement's footprint.

**C. Terrain creates ASYMMETRY between maps.**
Coastal map → linear settlement along shoreline, vulnerable flank is the sea. Valley map → concentrated, easy to defend, limited space. Plains map → sprawling, lots of room, hard to defend perimeter. Every seed should suggest a different strategy.

**D. Activity changes terrain over time.** (The full circle.)
The civilization doesn't just adapt to the land — it reshapes it. Mining depletes mountains. Farming changes soil. Deforestation leaves stumps. Over-farming degrades fertility. The map at tick 50K should look visibly different from tick 0, and the player should be able to read the settlement's history from the terrain.

#### Feature tiers

**Core (must-have for the pillar to feel real):**
- Precomputed resource map at world-gen (the world knows where everything is from tick 0)
- Resources distributed by geography (stone near mountains, wood in forests, fertile soil near rivers)
- Settlement shape driven by resource/terrain layout, not just radial blob
- Terrain visibly changes from activity: mining leaves quarry pits, farming shows tilled soil, deforestation leaves stumps with slow regrowth
- Rivers as real barriers (can't build on water, crossing is slow/costly, bridges are a building type)

**Rich (makes it special, worth building once core is solid):**
- Water proximity bonus for farms (irrigation without explicit channels)
- Seasonal terrain effects: spring floods in river plains, winter ice on water, dry summer fire risk
- Soil degradation from over-farming → forces crop rotation or expansion to new land
- Chokepoint detection → auto-build prefers garrison placement at narrow passes
- Elevation advantage for defense (higher ground = sight range bonus, combat bonus)
- Forest fire spread in dry seasons → clears land, creates opportunity and danger

**Dream (research projects, future deep-dives):**
- Dams: player places dam building → blocks water flow → creates reservoir upstream, dries downstream, enables irrigation
- River meandering over geological time (shallow water equations, could be its own project)
- Erosion reshaping terrain from sustained rainfall/water flow (we have a pipeline version; runtime version is the dream)
- Ice ages / climate shifts over very long timescales → biome boundaries move, forcing migration
- Aquifer system: underground water table, wells, depletion from over-mining
- Volcanic activity on certain seeds → fertile soil but eruption risk

#### Brainstorm: things to investigate

These aren't committed features — they're ideas worth prototyping or researching:

- **Fog of war / exploration map**: villagers only "see" terrain they've visited. Unexplored areas are dark. Creates genuine discovery moments.
- **Resource quality tiers**: not just "stone" but rich veins vs poor veins. Mountains near volcanoes have better ore.
- **Terrain-specific buildings**: fishing hut (water-adjacent only), mine entrance (mountain-adjacent), lumber mill (forest-adjacent). Placement IS the instruction, but constrained by geography.
- **Seasonal river flow**: rivers swell in spring (snowmelt), shrink in summer. Flood plains are fertile but dangerous.
- **Wind direction**: affects fire spread, rain shadow (already in pipeline), maybe windmill placement.
- **Sound/visual atmosphere per biome**: marsh has different ambient feel than tundra. Even in ASCII, this could be done with particle effects, color shifts.
- **Geological survey building**: once built, reveals resource locations in a radius. Bridges the gap between "precomputed resources" and "exploration discovery."

### Pillar 2: Emergent Complexity from Simple Agents

**The problem today:** Every villager is omniscient about the settlement (knows exact stockpile counts, all build sites, all deposit locations) but blind beyond sight range. There's no local reasoning, no information spread, no chain reactions between systems. A drought directly multiplies farm yield by 0.5 instead of going through water → soil → crop growth. Behaviors don't interact — a villager gathering wood has no effect on a villager farming.

**What emergence actually requires:**

**A. Agents respond to local conditions, not global state.**
Villagers shouldn't check `stockpile_wood < 10` (a global number). They should see that the wood pile near them is empty, or notice other villagers carrying wood in a certain direction. Local decisions + limited information = surprising aggregate behavior.

**B. Systems affect each other through the world, not through code.**
A drought shouldn't `farm_yield *= 0.5`. It should reduce water levels → reduce soil moisture → slow crop growth → less food at harvest. When we later add irrigation, it automatically mitigates drought because it raises water levels — not because we wrote `if drought && irrigated`. The simulation IS the logic.

**C. Simple rules, complex outcomes.**
Each agent rule fits in one sentence. "If hungry, seek food." "If carrying resources, go to stockpile." "If night, go home." Complexity comes from many agents following simple rules simultaneously in a shared environment with limited resources.

**D. Failure modes are emergent too.**
Settlements don't die from a "starvation event." They die because: nearby forests were cut → villagers walk further for wood → away longer → fewer farmers → less food → hunger → too weak to gather → death spiral. Every link is visible and logical.

#### Agent Knowledge Architecture

The core system that makes emergence real: **information is local, spreads through the world, not through global variables.**

**Layer 1: What I can see right now** (per-villager, per-tick)
- Terrain, entities, resources within sight range
- The building I'm in or next to
- Drives immediate behavior only (flee, eat, gather what's visible)

**Layer 2: What I personally remember** (per-villager, persistent, fades)
- Where my home is (assigned hut)
- Where the stockpile is (I've been there)
- Resources I've seen (there was a forest to the north — but it might be cut down now)
- Danger I've encountered (wolves came from the east)
- Memory decays or goes stale over time

**Layer 3: What the settlement knows** (shared, spreads through contact)
- A villager discovers a stone vein → returns to stockpile → settlement "knows"
- Other villagers learn when they visit the stockpile or encounter the discoverer
- Ant colony pheromone model: information spreads through the world, not global variables
- Buildings can act as information hubs (stockpile = bulletin board)

**Layer 4: What's actually true** (ground truth, precomputed at world-gen)
- The full resource map, terrain features, everything
- Villagers NEVER access this directly — they discover it through Layer 1, share through Layer 3

**Target: Medium-to-full ant colony model.** Villagers share info when near each other or at shared buildings. Information has timestamps and goes stale. Exploration is genuinely valuable because only explorers know what's out there. Environmental traces (worn paths, resource markers, territorial markings) supplement explicit communication. Knowledge becomes a resource — losing an experienced explorer means losing knowledge.

**Key implication:** Building a road to a distant resource doesn't just make travel faster — it makes information flow faster because more villagers pass through.

#### Feature tiers

**Core:**
- Per-villager memory (home location, known resources, visited locations)
- Villagers only seek resources they've personally seen or learned about at stockpile
- Stockpile as "bulletin board" — drop off resources AND pick up knowledge of what others found
- Sight-range-only awareness for immediate decisions (flee, gather, eat)
- Systems chain through simulation state, not hardcoded multipliers (drought → water → soil → crops)

**Rich:**
- Information sharing on encounter (two villagers meet → exchange knowledge)
- Memory decay (old info becomes unreliable, motivates re-scouting)
- Environmental traces: paths wear from foot traffic (already have traffic map), resource sites get marked
- Danger memory: villagers avoid areas where they saw predators recently
- Building-as-hub: different buildings share different info (garrison shares threat intel, workshop shares resource locations)

**Dream:**
- Full pheromone system: villagers leave chemical-like traces that attract/repel others
- Collective intelligence emerges from trace patterns (ant highway forms to rich resource)
- Cultural memory: settlement "remembers" things even after individuals die (through built environment, worn paths)
- Misinformation: stale knowledge causes villagers to walk to depleted resources, creating visible inefficiency that the player can solve by building roads/outposts
- Specialization from knowledge: villagers who explore a lot become scouts, villagers who farm a lot know the best soil

#### Brainstorm: things to investigate

- **"Bulletin board" data structure**: what does shared knowledge at a stockpile actually look like in code? A list of (resource_type, location, timestamp, reporter)?
- **Memory capacity**: should villagers forget old locations? Fixed-size memory (remember last N things)?
- **Information radius on buildings**: how far does "settlement knowledge" radiate from a stockpile?
- **Visual indicators of knowledge**: can we show what a villager knows? Overlay mode showing explored vs unexplored per-villager?
- **How do villagers decide what to share?** Do they dump everything at the stockpile, or only share when asked (another villager is idle and looking for work)?
- **Emergent scouting**: if exploration is valuable, will villagers naturally specialize into scouts? Or do we need to nudge that?

---

### Pillar 3: Explore → Expand → Exploit → Endure

**The problem today:** There is no arc. Population goes up, resources accumulate (or you die in winter), and it flatlines. The most interesting moment is always the first winter. After that, nothing changes. Every tick feels the same as the last.

**What this pillar means:**

The game has a natural emotional arc — anxiety → ambition → pride → tension — but it's NOT enforced by phases, timers, or locked features. It emerges from the simulation. The player feels the shift because *villager behavior naturally changes* as the world state changes. Nothing is gated. A player can explore and farm simultaneously, push expansion early, or turtle and exploit nearby resources. The "phases" describe what *tends to happen* on most maps, not what's forced.

**The arc as a gradient, not a state machine:**

**Explore** — The settlement is small and the world is unknown. Most villagers are discovering terrain, finding resources, mapping the landscape. Tension: "what kind of map did we get?" Payoff: "there's a stone vein in those mountains!"

Emerges naturally because: villagers can't gather what they haven't found. The knowledge architecture (Pillar 2) means early game IS exploration — there's nothing else to do until you know where things are.

Transitions when: enough resources are discovered that building becomes the priority. Not a switch — exploration tapers as known opportunities increase.

**Expand** — The settlement knows its landscape and is growing toward valuable resources. Buildings go up, outposts form, roads appear from traffic. Tension: "am I spreading too thin?"

Emerges naturally because: discovered resources create pull. Alluvial soil near the river → farms go there. Stone in the mountains → mining outpost. The settlement's shape is the terrain's shape.

Transitions when: the settlement's footprint stabilizes because available land is claimed and routes are established.

**Exploit** — Production chains are running. Workshops, smithies, bakeries. The satisfaction is watching the machine hum. But resources start depleting — forests thin, stone veins dry up, soil degrades.

Emerges naturally because: expansion creates infrastructure, infrastructure enables processing, processing creates efficiency loops. Depletion happens because resources are finite (Pillar 1 — terrain changes from activity).

Transitions when: depletion and threats shift the dominant concern from growth to survival.

**Endure** — Threats escalate. Easy resources are gone. The question shifts from "how do I grow" to "can I hold what I've built?" Losing a garrison to a raid actually hurts.

Emerges naturally because: large settlements attract threats (wolves scale with pop), depleted nearby resources force longer and more dangerous gathering trips, winter hits harder when supply lines are stretched.

**Key principle:** A player can be in multiple "phases" simultaneously. Half the settlement is exploiting while the other half explores a new direction. An aggressive player rushes expansion and deals with thin defenses. A cautious player over-invests in endurance and grows slowly. Both are valid.

#### Feature tiers

**Core:**
- Early game feels different from late game (villager state distribution visibly shifts over time)
- Resource depletion creates natural pressure to expand (forests thin, deposits empty)
- Threats scale with settlement size/wealth, not with a timer
- Discovery moments feel rewarding (finding a resource-rich area after long exploration)

**Rich:**
- Seasonal pressure creates rhythm within the arc (spring = expand, summer = exploit, autumn = prepare, winter = endure)
- Outpost mechanics: small satellite settlements near distant resources, connected by roads
- Trade-off visibility: player can see "I have 10 explorers and 3 farmers, that's risky" from the diagnostics overlay
- Milestone notifications that name what happened, not what phase you're in ("First stone deposit discovered" not "Entering Phase 2")

**Dream:**
- Multi-year arcs: Year 1 is survival, Year 3 is expansion, Year 10 is empire management
- Civilizational memory: the settlement's history is readable from the terrain (old quarry pits, abandoned outposts, ancient roads)
- Migration events: another settlement's refugees arrive, bringing knowledge but needing resources
- Legacy: when a settlement fails, its ruins become terrain features on the map for the next civilization

#### Brainstorm: things to investigate

- **What makes discovery feel good?** Is it enough to reveal terrain? Or does the villager need to "report back" and the player sees the knowledge appear on the map?
- **How do we make depletion visible and not just frustrating?** Stumps where forests were. Empty quarry pits. Fallow fields. The map tells the story of what was consumed.
- **Threat scaling formula**: linear with population? With territory size? With wealth (total resources)? With visibility (explored area attracts attention)?
- **What's the "endure" threat?** Wolves alone might not be enough. Climate shifts? Resource competition from rival settlements? Disease from overcrowding?
- **How long should a "full game" be?** 30K ticks? 100K? Should the player decide, or should the game have natural endpoints?

### Pillar 4: Observable Simulation

**The problem today:** Visual noise. Colors and ASCII characters fight each other — you're processing two channels and they compete instead of reinforcing. Entities are just `V` and you can't tell what they're doing by looking. Vegetation doesn't read as vegetation. The terrain is artificially smooth — rounded blobs, not geological. The lighting is actually good (stepped looks great in terminal) but it's applied to symbolic characters that don't benefit from it. The result: too much information, not enough clarity. You can't quickly parse what's happening.

**The deeper problem:** The terminal aesthetic has unrealized magic. "Peeking into a real world through a terminal window" is a powerful fantasy. But right now the visuals are in an uncanny valley — too detailed to be clean ASCII, too crude to be a real landscape. Trying to be both, failing at each.

**The insight: two rendering philosophies, not one.**

Since the renderer is hot-swappable, build two modes with distinct philosophies. The player toggles between them. Each mode is internally consistent — no Frankenstein mixing.

#### Mode A: "Map" — Clean Symbolic ASCII

**Philosophy:** Characters carry ALL the meaning. Color is minimal.

- Distinct glyphs per terrain: `♠` tree, `▲` mountain, `~` water, `·` grass, `,` sand, `*` stone deposit
- Distinct glyphs per entity: `○` villager (or directional: `→` `←` `↑` `↓`), `●` wolf, `◊` prey, `⌂` hut
- Color is flat per-biome: green = fertile, brown = dry, blue = water, grey = mountain. No lighting.
- Entities POP because they're unique characters on a clean background
- Glance at any tile → instantly know what it is
- Dwarf Fortress energy. Gameplay mode. "What's happening, where are my villagers."

#### Mode B: "Landscape" — Painterly Terminal

**Philosophy:** Color carries ALL the meaning. Characters are texture.

- Characters are noise/texture that suggest surface: `.` `'` `,` `:` `"` — NOT semantic symbols
- Different texture densities for different terrain (sparse `.` for sand, dense `"♣` for forest)
- Full lighting and shadow system — this is where the stepped lighting SHINES
- Color gradients across elevation, biome, moisture — you read the color field, not individual characters
- Entities are bright saturated characters against muted terrain palette — they stand out by color contrast
- Weather particles, season tinting, atmospheric fog in distance
- The "peek into a real world through a terminal window" vibe. Watch mode. "This is beautiful."

#### Mode B+: "Landscape" advanced rendering (future)

Mode B is where we push the terminal rendering envelope:
- **Parallax/depth**: distant terrain uses muted colors, nearby is vivid. Fog of distance.
- **Texture variety**: multiple character sets per terrain type, randomized per-tile for organic look
- **Water animation**: cycling characters `~≈∼` for flowing water, still characters for lakes
- **Shadow casting**: buildings and mountains cast directional shadows based on sun position (we have this — refine it)
- **Particle systems**: rain, snow, embers from fire, dust in wind
- **Light sources**: torches at buildings glow warm at night, create pools of light
- **Seasonal atmosphere**: spring is bright/green, summer is warm/golden, autumn is orange/red, winter is blue/grey with white particles
- **Camera effects**: slight color shift at edges, vignette in landscape mode

#### The toggle

Player presses `v` (already exists for view cycling):
- Map mode: instant readability, gameplay decisions
- Landscape mode: atmospheric, watch the world live
- Debug mode (existing): raw terrain types, no rendering

Both modes show the same simulation. Map mode is where you play. Landscape mode is where you fall in love.

#### Feature tiers

**Core:**
- Two distinct rendering modes that don't fight each other
- Map mode: clean glyphs, flat color, entities are instantly readable
- Landscape mode: texture characters, full lighting, color-dominant
- Entity visibility: in both modes, you can tell what a villager is DOING (carrying, building, exploring, fleeing) from their appearance

**Rich:**
- Entity state shown visually: villager carrying wood looks different from idle villager. Fleeing villager faces away from threat. Builder has a distinct animation cycle.
- Activity indicators: smoke from workshops, dust from construction, sparkle from mining
- Resource flow visibility: paths between stockpile and resource sites are worn/visible (traffic map → visual)
- Threat visibility: wolf territory shown as subtle color shift, approaching pack visible from distance

**Dream:**
- Mode B+ full atmospheric rendering (parallax, animated water, light sources, particles)
- Zoom levels: zoomed out = each character is a region summary, zoomed in = individual tile detail
- Cinematic mode: camera slowly pans across the settlement, auto-framing interesting activity
- Screenshot mode that renders at high quality for sharing (we have basic PNG — make it beautiful)
- Sound design (terminal beeps/tones for events — yes, really, terminal audio exists)

#### Brainstorm: things to investigate

- **What characters best suggest terrain texture without being semantic?** Need to test character sets on actual terminal with actual colors.
- **How do we show villager intent visually?** Color coding (red = fleeing, green = gathering, yellow = building)? Trailing particle? Direction of glyph?
- **Half-block rendering**: some terminals support `▀▄` half-block characters for double vertical resolution. Worth exploring for landscape mode — effectively doubles our pixel count.
- **Braille characters**: `⠁⠃⠇⠏⠟⠿⡿⣿` — 2x4 dot matrix per character cell. Some terminals support these. Could enable near-pixel-level rendering in landscape mode.
- **Reference art**: what does the BEST terminal art look like? Search for terminal demos, ASCII art landscapes, ANSI art competitions. What techniques do they use?
- **Color palette design**: curate specific palettes per biome/season rather than computing colors algorithmically. Hand-picked colors look better than math.

### Pillar 5: Scale Over Fidelity

**The problem today:** 30 villagers is fine. 500+ is the goal. 1000+ is the dream. Current architecture won't survive — every villager runs A* every tick, scans all entities for sight-range checks, and shares global state. That's O(villagers * entities) per tick and O(villagers) A* calls per frame.

**The bottlenecks and solutions:**

#### A. AI computation — data-oriented + spatial partitioning

**Problem:** `ai_villager()` checks distance to every food source, stone deposit, build site, hut, and predator. O(villagers * entities) per tick.

**Solution — spatial hash grid:** Chunk the map into cells (16x16 or 32x32). Each cell tracks which entities are in it. "Find entities near me" becomes O(nearby_cells) not O(all_entities). This is the single highest-impact optimization.

**Solution — data-oriented layout:** Currently entities are hecs components scattered in memory. For hot-path operations (position checks, sight range scans), we want contiguous arrays — positions packed together, behaviors packed together. Consider a parallel flat array for "AI-relevant state" that we sync from hecs each tick. Cache-friendly iteration at scale.

**Solution — tick budgeting:** Not every villager thinks every tick. Distant/offscreen villagers think every 3-5 ticks. Idle villagers think every 5-10 ticks. Active/nearby villagers think every tick. Cuts AI cost 3-5x and players don't notice.

#### B. Pathfinding — memoization + hierarchical

**Problem:** Every seeking/exploring villager runs A* every tick on a 256x256 grid. 200 simultaneous pathfinders = bad.

**Solution — memoize/cache paths:** A villager walking to a known destination doesn't need to re-pathfind every tick. Compute path once, follow it, only recompute if blocked or destination changes. Store as Vec<(x,y)> waypoints.

**Solution — hierarchical pathfinding:** Precompute a coarse navigation mesh — divide map into regions, know which regions connect. Villagers pathfind region-to-region (cheap), then do local A* within each region (small grid). Only recompute nav mesh when terrain changes (building placed, tree cut).

**Solution — flow fields:** For common destinations (stockpile, popular resources), precompute a flow field that all villagers can share. Instead of 50 villagers each running A* to the stockpile, compute one flow field and they all read from it. Amortized cost.

#### C. Rendering — per-tile not per-entity

**Problem:** 500+ entities means O(entities) visibility checks per frame. Overlapping entities in crowded areas are visual noise.

**Solution — render per-tile:** Instead of iterating entities and drawing each one, iterate visible tiles and draw the "most important" entity on each tile. Tile already occupied by a building? Don't draw the 5 villagers standing on it. Crowded area? Show density indicator, not 20 overlapping Vs.

**Solution — LOD for agents:** At zoomed-out views, don't render individual villagers. Show activity heat maps, population density dots, or aggregate indicators. "50 villagers are farming this area" as a colored region, not 50 characters.

**Solution — dirty-rect rendering:** Only redraw tiles that changed since last frame. Most tiles are static terrain — only entity positions and weather change. Could cut render cost dramatically.

#### D. Knowledge/communication — group chunking

**Problem:** Pillar 2's knowledge architecture means villagers share info on contact. Naive approach: O(villagers^2) encounter checks per tick.

**Solution — spatial grid again:** Same grid from (A) solves this. Only check encounters within the same cell. O(villagers_per_cell^2 * num_cells) which is much smaller.

**Solution — group/flock abstraction:** When many villagers are doing the same thing in the same area (10 farmers in the same field), treat them as a "group" for communication purposes. Info shared with the group propagates to all members. Similar to They Are Billions' army grouping — individual simulation for spread-out agents, group simulation for clusters. The transition should be seamless.

**Solution — building-mediated communication:** Most info sharing happens at buildings (stockpile bulletin board from Pillar 2). Buildings have a fixed, small number. Villagers check the building's knowledge when they visit, not when they encounter each other. Scales with building count, not villager count.

#### E. World simulation at scale

**Problem not yet felt but will matter:** Water simulation, vegetation growth, moisture propagation all iterate the full map. 256x256 = 65K tiles is fine. 512x512 = 262K tiles gets expensive per tick.

**Solution — simulation LOD:** Run expensive simulations (water flow, erosion) at lower frequency — every 10 or 50 ticks instead of every tick. Vegetation updates only near active areas. Chunk-based updates — only simulate chunks that have changed or have entities in them.

#### Performance targets

| Population | Target FPS | AI budget/tick | Path budget/tick |
|------------|-----------|----------------|------------------|
| 30 (current) | 60 | no budget needed | no budget needed |
| 100 | 60 | 2ms | 1ms |
| 500 | 60 | 5ms | 3ms |
| 1000 | 30-60 | 8ms | 5ms |

#### Feature tiers

**Core:**
- Spatial hash grid for entity lookups (fixes AI, rendering, communication)
- Path caching (compute once, follow waypoints, recompute on blocked)
- Tick budgeting (idle/distant villagers think less often)
- Per-tile rendering with entity priority

**Rich:**
- Hierarchical pathfinding (nav mesh + local A*)
- Flow fields for common destinations (stockpile, popular resources)
- Group/flock abstraction for clustered villagers
- Data-oriented parallel arrays for hot-path AI data
- Dirty-rect rendering

**Dream:**
- 1000+ agents at 60fps
- 512x512 or larger maps with chunked simulation
- LOD agent rendering (individual → density heat map based on zoom)
- GPU-accelerated terminal rendering (if terminal supports it)
- Deterministic simulation for replay/debugging (fixed-point math, seeded RNG per entity)

#### Brainstorm: things to investigate

- **What's the actual performance profile today?** We should profile before optimizing — is it AI, pathfinding, rendering, or something unexpected?
- **hecs performance at scale**: does hecs handle 1000+ entities well, or do we need to consider a different ECS? (hecs is generally fast but worth benchmarking)
- **Spatial hash cell size**: 16x16? 32x32? Depends on sight range (currently 22 tiles). Cells should be roughly sight-range sized.
- **How does They Are Billions actually handle thousands of zombies?** Worth researching their specific technique for group simulation.
- **Flow fields vs A***: at what entity count does flow field become cheaper? Probably ~20 agents sharing a destination.
- **Can we parallelize AI?** Each villager's AI is mostly independent (reads world state, produces new state). Rayon parallel iteration over villagers? Need to handle the mutable world state carefully.

---

## Open Questions

- Should villagers have any individual state beyond hunger/position/behavior? (Skills per-villager vs per-civilization?)
- How deep should water interaction go? (Irrigation? Fishing? River crossing penalties?)
- Should the player ever directly place anything other than buildings? (Designate mining areas? Set patrol routes?)
- Is there a z-level / elevation gameplay layer worth pursuing?
- Tower defense project — share a harness/engine, or keep separate and extract patterns later?
- What's the right balance between "realistic geology simulation" and "fun game with interesting terrain"?
