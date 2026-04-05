# Game Systems Survey: State of the Art for Settlement/Colony Simulation

Research date: 2026-04-04. Focus: technically excellent, well-documented implementations
relevant to a Rust terminal-based settlement sim (terrain-gen-rust).

---

## 1. Agent AI

### How the big games do it

**Dwarf Fortress** does NOT use GOAP, behavior trees, or utility AI in the traditional
sense. Tarn Adams wrote a custom simulation where each dwarf has:
- 120 emotion glosses filtered through personality traits
- A memory system (dwarves remember past events and accumulate stress)
- A needs system (taverns, temples, libraries satisfy needs; unmet needs cause spirals)
- Relationships (family, friends, grudges, citizenship, religion)
- Jobs assigned by a priority queue; dwarves pick the highest-priority available task

The key insight from Tarn Adams' Game AI Pro 2 chapter ("Simulation Principles from
Dwarf Fortress") is four principles:
1. Don't overplan models -- let surprising results emerge
2. Dissect systems into core components for richer causal linkages
3. Limit complexity to essential variables for practical tuning
4. Anchor simulations in real-world physical/biological analogs

Source: [Game AI Pro 2 Ch.41](http://www.gameaipro.com/GameAIPro2/GameAIPro2_Chapter41_Simulation_Principles_from_Dwarf_Fortress.pdf)
Quality: Nick-level (this is THE canonical reference)

**RimWorld** uses a priority-based job system:
- Each pawn has Work priorities (1-4) and a left-to-right ordering of work types
- WorkGiver classes (especially WorkGiver_Scanner) find available tasks
- Region system (max 16x16 tiles) enables O(1) nearest-work lookups
- ThinkTree structure defines behavior fallback chains
- No GOAP/utility -- just strict priority with region-based spatial queries

Source: [RimWorld AI Tutorial Wiki](https://github.com/CBornholdt/RimWorld-AI-Tutorial/wiki/Part-1---Introduction), [Decompiled source](https://github.com/Chillu1/RimWorldDecompiled)
Quality: Polished (battle-tested in one of the most popular colony sims)
Applicability: HIGH -- this priority+region model is simple enough for our terminal game

**Songs of Syx** handles 30,000 individual people by being heavily CPU-bound with
simplified per-agent logic. No detailed public documentation of their AI architecture,
but the scale proves that simple-per-agent + efficient data layout = thousands of agents.

Source: [Songs of Syx](https://songsofsyx.com/)
Quality: Battle-tested at scale
Applicability: MEDIUM -- proves the "keep it simple per agent" approach works

### Open-source implementations in Rust

**big-brain** -- Utility AI library for Bevy. Scorers evaluate world state into numeric
scores, Actions execute behaviors, Thinkers compose them. Highly parallel via Bevy's
scheduler. ARCHIVED on GitHub (moved to Codeberg). 1.3k stars.
- Scorers = "eyes" that produce Score values
- Actions = behaviors with state machine (Requested/Success/Cancelled/Failure)
- Thinkers = decision pickers (FirstToScore with thresholds)

Source: [github.com/zkat/big-brain](https://github.com/zkat/big-brain) (now on Codeberg)
Quality: Decent (well-designed API, but tied to Bevy)
Applicability: MEDIUM -- good design reference but we'd need to extract patterns, not use directly

**bonsai-bt** -- Behavior tree library for Rust. Engine-agnostic. Supports sequences,
selectors, decorators. Has drone and NPC examples. Derived from the Piston game engine
ecosystem.

Source: [github.com/Sollimann/bonsai](https://github.com/Sollimann/bonsai), [crates.io](https://crates.io/crates/bonsai-bt)
Quality: Decent (clean API, good examples)
Applicability: HIGH -- engine-agnostic, could integrate with our ECS

**DwarfCorp** -- Open-source C# colony sim. Started with FSMs, tried GOAP (failed at
scale), settled on behavior trees ("Acts"). The GOAP-to-BT migration story is instructive:
they extracted GOAP "meta-actions" and restructured them as BT nodes. Modified MIT license.

Source: [github.com/Blecki/dwarfcorp](https://github.com/Blecki/dwarfcorp), [GDC post](https://www.gamedeveloper.com/programming/how-we-developed-robust-ai-for-dwarfcorp)
Quality: Polished (years of iteration, well-documented journey)
Applicability: HIGH -- the FSM->GOAP->BT progression is exactly the pitfall to avoid

### Recommended approach for terrain-gen-rust

RimWorld's model: **priority-based job queue + spatial region system**. Skip GOAP (too
expensive for 1000+ agents). Skip full utility AI (overkill for deterministic needs).
Use behavior trees only for complex multi-step tasks (building, combat). Keep the base
loop as: pick highest-priority available job in nearest region, execute it.

For personality/stress: follow DF's model of a small set of needs (food, shelter, social,
purpose) with memory of recent events that shift a stress accumulator. 120 emotions is
aspirational; start with 10-15.

---

## 2. Economy Systems

### Victoria 3's economic model

The most sophisticated game economy ever built. Key architecture:
- **Open market system**: goods flow through supply/demand, prices emerge from transactions
- **Discrete weekly ticks**: money flows through pipes from A to B to C each tick
- **MAPI** (Market Access Price Impact): local prices affected by market connectivity
- **Population-driven demand**: 700+ regions, up to 100k pop groups, each with consumption needs
- **No player-directed distribution**: consumers bid for goods based on funds and prices

Source: [GDC Deep Dive](https://www.gamedeveloper.com/design/deep-dive-modeling-the-global-economy-in-victoria-3)
Quality: Nick-level (the gold standard for game economies)
Applicability: HIGH for design philosophy, MEDIUM for implementation (we need a much simpler version)

### BazaarBot -- Open-source economics engine

Agent-based free market simulator. The key algorithm:
- Each agent maintains a **price belief range** [low, high] per commodity
- When buying/selling, agents pick a random price within their belief range
- **Market clearing**: shuffle bids (highest first) and asks (lowest first), match sequentially
- Trade executes at average of bid/ask price
- After each round: successful trades narrow belief range, failed trades widen it
- **Bankrupt agents replaced** by agents of the most profitable job type (emergent career distribution)

Based on "Emergent Economies for Role Playing Games" (Doran & Parberry).

Source: [github.com/larsiusprime/bazaarBot](https://github.com/larsiusprime/bazaarBot) (Haxe, MIT, 387 stars)
Quality: Decent (clean proof of concept, but no formal API)
Applicability: HIGH -- this is exactly the right complexity level for our game
Ports exist in: JavaScript, Java, Python. No Rust port yet (opportunity).

### Key paper: "Emergent Economies for Role Playing Games" (Doran & Parberry)

The foundational paper behind BazaarBot. Agents maintain price beliefs as ranges,
update them based on trade success/failure. Production chains create interdependencies.
Population dynamics emerge from agent profitability.

Source: [Paper PDF](https://ianparberry.com/pubs/econ.pdf)
Quality: Polished (seminal academic work, widely cited)
Applicability: HIGH -- should be our primary reference for economy implementation

### Rust-specific tools

**krABMaga** -- Agent-based modeling framework in Rust (adapted from MASON). Good for
prototyping economy simulations but not game-specific.

Source: [github.com/krABMaga/krABMaga](https://github.com/krABMaga/krABMaga)
Quality: Decent
Applicability: LOW (research tool, not game engine)

**bourse-de** -- Discrete event market simulation crate. Order-book based.

Source: [crates.io/crates/bourse-de](https://crates.io/crates/bourse-de)
Quality: Proof-of-concept
Applicability: LOW (financial markets, not game economies)

### Recommended approach for terrain-gen-rust

Port BazaarBot's price belief model to Rust. Start with 5-8 commodities (food, wood,
stone, ore, tools, cloth, pottery, luxury). Each settlement agent has price beliefs.
Weekly market clearing. Bankrupt agents switch professions. This gives emergent
specialization without Victoria 3's complexity.

---

## 3. Weather/Climate Simulation

### Nick McDonald's procedural weather

Grid-based cellular automaton. Per-cell variables: wind speed, temperature, humidity,
cloud presence, precipitation. Physical coupling:
- Wind from Perlin noise (time-dependent global vector), modified by terrain slope
- Temperature: decreased by rain, adiabatic cooling uphill, solar heating where no clouds
- Humidity: evaporation from water bodies (higher at higher temp), removed by rain, transported by wind
- Cloud/rain: condensation above threshold temp+humidity values
- Diffusion: cells average with neighbors after convective transport

Source: [nickmcd.me/2018/07/10/procedural-weather-patterns/](https://nickmcd.me/2018/07/10/procedural-weather-patterns/)
Quality: Nick-level
Applicability: HIGH -- we already use Nick's hydrology work; this extends naturally

### One Wheel Studio's environmental simulation

Two-layer atmosphere model (ground + upper atmosphere per block). Key additions beyond Nick:
- **Seasonal cycles**: sinusoidal curve for sun intensity (summer peak, winter trough)
- **Daily flux**: separate sinusoidal for day/night temperature variation
- **Terrain-specific median temperatures**: ocean, grassland, mountains each have base temps
- **Wind from temperature gradients**: compute temp difference between neighbors, multiply by
  direction vector, sum all neighbors. Hot blows toward cold.
- Runs off main thread for performance

Source: [onewheelstudio.com/blog/2017/4/1/environmental-simulation](https://onewheelstudio.com/blog/2017/4/1/environmental-simulation)
Quality: Decent (game-focused, practical)
Applicability: HIGH -- the seasonal sinusoid + terrain median approach is perfect for our grid

### 2D Weather Sandbox (niels747)

Real-time interactive troposphere simulation in the browser. Full fluid dynamics for
a 2D cross-section of atmosphere. Too expensive for a game but excellent reference for
understanding what the cheap models approximate.

Source: [github.com/niels747/2D-Weather-Sandbox](https://github.com/niels747/2D-Weather-Sandbox)
Quality: Polished (interactive, educational)
Applicability: LOW (too expensive) but HIGH for understanding

### Joe Duffy's climate simulation

Uses Perlin noise for rainfall and temperature, adjusted by latitude (equator = hot/dry)
and altitude (higher = colder, less rain). Simple but effective for world-gen.

Source: [joeduffy.games/climate-simulation-for-procedural-world-generation](https://www.joeduffy.games/climate-simulation-for-procedural-world-generation)
Quality: Decent
Applicability: MEDIUM -- good for initial world gen, but we want dynamic weather too

### Recommended approach for terrain-gen-rust

We already have Stam stable fluids for wind. Layer on:
1. Per-cell humidity (evaporation from water, transported by wind, removed by rain)
2. Temperature with seasonal sinusoid + altitude + latitude adjustment
3. Cloud/rain threshold on humidity+temperature
4. Run weather update every N game ticks (not every frame)

This gets us dynamic weather cheaply on top of existing wind infrastructure.

---

## 4. Procedural Narrative/Events

### Dwarf Fortress: pure emergent narrative

DF has ZERO embedded narratives. Stories emerge entirely from simulation:
- Dwarves have personalities (traits affect responses to events)
- Memory system: dwarves accumulate memories that shift stress
- Needs system: unmet needs (social, artistic, religious) cause behavioral spirals
- Biographies track life history, relationships, moods, skills
- Rumors propagate through social networks
- World history generates civilizations, wars, artifacts during world-gen

The key insight: DF doesn't WRITE stories. It simulates a world with enough detail
that players READ stories into the output. The simulation IS the narrative engine.

Source: [Emergent Narrative in Dwarf Fortress (book chapter)](https://www.taylorfrancis.com/chapters/edit/10.1201/9780429488337-15/emergent-narrative-dwarf-fortress-tarn-adams),
[GDC interview](https://www.gamedeveloper.com/design/q-a-dissecting-the-development-of-i-dwarf-fortress-i-with-creator-tarn-adams)
Quality: Nick-level (the gold standard for emergent narrative)
Applicability: HIGH -- this is our north star

### RimWorld: storyteller-driven event system

RimWorld takes the opposite approach -- a director AI paces events:

**Architecture** (from decompiled source):
- `StorytellerComp_RandomMain`: rolls incidents using category weights + mtbDays (mean time between)
- `StorytellerComp_OnOffCycle`: schedules events on cyclical patterns
- Incident categories: ThreatBig, ThreatSmall, Disease, OrbitalVisitor, AllyArrival, Misc
- `PopulationIntent`: adjusts colonist-adding event probability based on current pop vs target
- Threat points scale with colony wealth (richer = harder raids)
- Each incident has a `baseChance` weight; storyteller picks category first, then weighted-random within category

The three storytellers (Cassandra, Phoebe, Randy) differ only in their StorytellerComp
parameter tuning -- same engine, different knobs.

Source: [RimWorld Wiki - AI Storytellers](https://rimworldwiki.com/wiki/AI_Storytellers),
[Decompiled source](https://github.com/Chillu1/RimWorldDecompiled),
[Storyteller Enhanced mod](https://github.com/Lanilor/Storyteller-Enhanced)
Quality: Polished (proven across millions of players)
Applicability: HIGH -- the StorytellerComp architecture is directly portable

### Three techniques for procedural storytelling (Davide Aversa)

Overview comparing: simulation-based (DF), director-based (RimWorld/Left4Dead),
grammar-based (Tracery/generative text). Good taxonomy.

Source: [davideaversa.it/blog/overview-procedural-storytelling](https://www.davideaversa.it/blog/overview-procedural-storytelling/)
Quality: Decent (clear overview)
Applicability: MEDIUM (taxonomy, not implementation)

### Recommended approach for terrain-gen-rust

Hybrid: DF-style emergent narrative from simulation (needs, memory, personality) PLUS
RimWorld-style storyteller pacing for external events (raids, traders, weather disasters).
The storyteller prevents boring stretches and ensures difficulty scaling. The simulation
generates the personal stories.

Start with:
1. Event queue with weighted-random category selection (port RimWorld's StorytellerComp)
2. 3-5 event categories: Threat, Trade, Migration, Weather, Discovery
3. Colony wealth drives threat scaling
4. Per-agent needs + memory (simplified DF model)

---

## 5. Pathfinding at Scale

### Algorithm comparison (benchmarked)

From the Grid Engine benchmarks and academic papers:

| Algorithm | Median (ms) | Notes |
|-----------|-------------|-------|
| A* | baseline | Standard, optimal paths |
| JPS | 7.19 | 10x faster than A* on grids |
| JPS+ | 2.5 | Pre-computed jump points |
| Hierarchical JPS | 1.5 | Best overall |
| HPA* | ~5x faster than A* | Chunk-based caching |
| Flow fields | one-time cost | Best for many agents to one target |

**JPS** (Jump Point Search): skips symmetric paths on uniform-cost grids. 10x+ faster
than A* with identical results. Only works on grids (not arbitrary graphs).

**HPA*** (Hierarchical Pathfinding A*): divides grid into chunks, caches inter-chunk
paths. 96% reduction vs A* in some benchmarks. Slightly suboptimal paths.

**Flow fields**: compute ONE direction field for entire map per destination. All agents
reuse it. StarCraft 2 uses this for army movement. Best when many agents share a destination.

Source: [Grid Engine perf comparison](https://annoraaq.github.io/grid-engine/p/pathfinding-performance/index.html),
[Red Blob Games flow fields](https://www.redblobgames.com/blog/2024-04-27-flow-field-pathfinding/),
[JPS paper](https://www.researchgate.net/publication/266007915_The_JPS_Pathfinding_System)
Quality: Nick-level (Red Blob Games is always excellent)

### Rust crates

**grid_pathfinding** -- JPS with improved pruning + connected components pre-check.
4-neighborhood and 8-neighborhood. On the dao/arena2 benchmark set (910 scenarios,
281x209 grid): JPS = 63.2ms total, A* = 702ms. 11x speedup.

Source: [github.com/tbvanderwoude/grid_pathfinding](https://github.com/tbvanderwoude/grid_pathfinding),
[crates.io](https://crates.io/crates/grid_pathfinding)
Quality: Decent (benchmarked, maintained)
Applicability: HIGH -- drop-in for grid pathfinding

**hierarchical_pathfinding** -- HPA* implementation in Rust. Chunk-based caching with
configurable chunk size. Parallel cache building via Rayon. When terrain changes, only
affected chunks recalculate.

Source: [github.com/mich101mich/hierarchical_pathfinding](https://github.com/mich101mich/hierarchical_pathfinding),
[crates.io](https://crates.io/crates/hierarchical_pathfinding)
Quality: Decent (well-documented, based on the HPA* paper)
Applicability: HIGH -- exactly what we need for large maps with dynamic terrain

**jps** crate -- Pure JPS implementation, supports HashMap-based maps.

Source: [crates.io/crates/jps](https://crates.io/crates/jps)
Quality: Proof-of-concept
Applicability: MEDIUM

### What colony sims actually use

- **RimWorld**: region-based (16x16 chunks) with A* between regions. Not the fastest
  algorithm but the region system makes work-finding O(1).
- **Dwarf Fortress**: custom pathfinding with connectivity caching. Known to be a
  performance bottleneck with large fortresses.
- **Songs of Syx**: unknown specifics but handles 30k agents (likely simplified movement).

### Recommended approach for terrain-gen-rust

Layer 1: **grid_pathfinding** (JPS) for individual agent pathfinding.
Layer 2: **hierarchical_pathfinding** (HPA*) for long-distance paths on large maps.
Layer 3: **RimWorld-style regions** for job assignment (find nearest work without pathfinding).
Layer 4: Flow fields for group movement (raids, migrations) -- implement ourselves, it's simple.

Start with JPS (Layer 1) + regions (Layer 3). Add HPA* when maps exceed 256x256.

---

## 6. Sound Design for Terminal Games

### Yes, terminal games can have sound

The key insight: terminal rendering and audio are completely independent systems.
A terminal game is just a Rust program -- it can play audio through any audio library
while rendering text to stdout.

### Ratatui + Rodio (proven combination)

A developer built a Ratatui terminal game with full sound effects using Rodio:
- Pre-buffer small sound files at startup (10KB each)
- Play through a `Sink` that queues playback sequentially
- Short, discrete sound effects work best (not ambient loops)
- Minimal dependency footprint: `rodio = { default-features = false, features = ["symphonia-mp3"] }`

Source: [Ratatui Audio with Rodio](https://dev.to/askrodney/ratatui-audio-with-rodio-sound-fx-for-rust-text-based-ui-bhd)
Quality: Polished (working code, clear tutorial)
Applicability: HIGH -- we already use Ratatui; this is a direct integration path

### Rust audio libraries

**Rodio** (github.com/RustAudio/rodio): Simple audio playback. Uses cpal underneath.
Supports MP3, WAV, OGG, FLAC. Good for sound effects.

**Kira** (github.com/tesselode/kira): Expressive game audio with tweens, mixer effects,
clock system for timing, spatial audio. More sophisticated than Rodio.

Source: [Are We Game Yet - Audio](https://arewegameyet.rs/ecosystem/audio/)
Quality: Both polished and well-maintained
Applicability: Rodio for v1 (simple SFX), Kira for v2 (ambient music, spatial audio)

### What sounds work in terminal games

- Discrete event sounds: hammering, chopping, mining, alerts, combat
- Ambient background: wind, rain, fire (subtle loops)
- UI feedback: menu navigation clicks, error beeps
- Music: lo-fi procedural or chip-tune style

The terminal's `\a` bell character exists but is terrible. Use real audio libraries.

### Recommended approach for terrain-gen-rust

1. Add `rodio` with minimal features to Cargo.toml
2. Pre-buffer 10-20 small sound effects at startup
3. Play sounds on game events (combat, building, alerts)
4. Optional: add ambient weather sounds tied to weather system
5. Keep it optional (--no-audio flag) for headless/CI usage

---

## Summary: Priority Implementation Order

Based on applicability and effort-to-impact ratio:

| Priority | System | Key Reference | Effort |
|----------|--------|---------------|--------|
| 1 | Pathfinding (JPS) | grid_pathfinding crate | Low -- drop-in crate |
| 2 | Job/AI system | RimWorld priority+regions | Medium -- core architecture |
| 3 | Event storyteller | RimWorld StorytellerComp | Medium -- weighted random + pacing |
| 4 | Economy | BazaarBot price beliefs | Medium -- port from Haxe to Rust |
| 5 | Weather | Nick McDonald + seasonal sinusoid | Low -- extends existing wind system |
| 6 | Agent personality | DF needs/memory/stress | Medium -- enriches emergent narrative |
| 7 | Sound | Rodio + Ratatui | Low -- optional enhancement |
| 8 | Hierarchical pathfinding | hierarchical_pathfinding crate | Low -- when maps get big |

The biggest bang-for-buck is #2 (job system) because it gates everything else -- agents
need to DO things before economy, narrative, or personality matter.
