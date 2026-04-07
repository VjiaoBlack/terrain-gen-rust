# Game Design Inspiration for Terminal-Based Settlement Simulation

Deep research into colony sims, roguelike UI, color theory, emergent narrative,
and modding systems. Focus: what is brilliant, what is underappreciated, and how
each idea applies to a terminal-rendered settlement sim.

---

## Table of Contents

1. [Colony Sims with Novel Mechanics](#1-colony-sims-with-novel-mechanics)
2. [Information Visualization in Terminal/ASCII](#2-information-visualization-in-terminalascii)
3. [Color Theory for Games](#3-color-theory-for-games)
4. [Sound Design for Ambient Simulation](#4-sound-design-for-ambient-simulation)
5. [Emergent Storytelling](#5-emergent-storytelling)
6. [Novel UI Patterns for Complex Sims](#6-novel-ui-patterns-for-complex-sims)
7. [Modding Systems](#7-modding-systems)
8. [Synthesis: Principles for terrain-gen-rust](#8-synthesis-principles-for-terrain-gen-rust)

---

## 1. Colony Sims with Novel Mechanics

### Songs of Syx -- Scale as the Mechanic

- **Source**: https://songsofsyx.com/ | https://www.realityremake.com/articles/songs-of-syx-review-a-brutally-complex-colony-sim-with-endless-replay
- **What makes it exceptional**: Solo developer achieved 50,000 individually-simulated
  citizens, each with needs, religion, fears, and desires. The trick: pixelated art
  style keeps GPU load minimal, shifting the bottleneck to CPU where simulation
  actually matters. The game is "mostly automated -- you let the AI do the mundane
  tasks while you focus on greater, more kingly things."
- **Brilliant design decision**: Abstracting individual micro-management into zone-based
  policy. You don't tell each citizen what to do; you define zones, priorities, and
  rules. The citizens become an emergent workforce. This is what lets it scale.
- **How it applies to terrain-gen-rust**: This is the direct aspiration. In terminal,
  we can't render 50K sprites, but we CAN simulate 50K agents. The terminal becomes
  a dashboard for the simulation, not a viewport for graphics. Zone-based control
  maps perfectly to terminal overlays (paint zones with keyboard, see them as colored
  regions). The key lesson: **the simulation IS the game; the renderer is just a
  window into it.**

### Oxygen Not Included -- Layered Challenge Architecture

- **Source**: https://www.gamedeveloper.com/design/behind-the-design-of-hit-sim-game-i-oxygen-not-included-i- | https://www.gamedeveloper.com/design/layering-challenges-in-klei-s-survival-sim-i-oxygen-not-included-i-
- **What makes it exceptional**: Every pixel on screen represents a limited resource.
  Gases sort by density (hydrogen floats, CO2 sinks). The closed-loop system means
  manipulating one resource cascades into others. Graham Jans' key insight: "No
  particular obstacle is supposed to be particularly difficult -- it's the layering
  that makes intense decisions."
- **Brilliant design decision**: The "solved problem" trap avoidance. Even after you
  get oxygen production working, suffocation remains a lurking threat. Nothing is
  ever permanently solved. Discovery-based learning: "a big part of the fun is the
  sense of figuring things out" -- so they resist tutorials.
- **How it applies**: Our terrain pipeline already simulates water flow, temperature,
  and geology. ONI proves that *interconnecting* these systems is what creates
  gameplay. If underground water meets hot magma, steam should form. If a food
  store floods, it should rot. **Layered challenges from physics, not scripted
  events.** Terminal overlay modes (temperature view, water view, food view) become
  essential to communicate these layers.

### Frostpunk -- Society as a System

- **Source**: https://brickwallpictures.medium.com/how-frostpunk-injects-harrowing-moral-choices-into-the-city-builder-genre-8536ad222cee | https://www.invenglobal.com/articles/6528/igc-2018-empathize-with-numbers-the-process-of-frostpunk-scenario-writing
- **What makes it exceptional**: The Book of Laws. Each law you sign makes the game
  mechanically easier but morally harder. Order vs. Faith isn't good vs. evil -- both
  paths slide into authoritarianism. The game never punishes you for choices; it
  simply shows you consequences. "Instead of trying to teach the player, Frostpunk
  gives him an opportunity to teach the world what he thinks is right."
- **Brilliant design decision**: Hope and Discontent as visible meters, not hidden
  stats. The emotional state of your society is quantified and front-and-center.
  When Hope hits zero, you lose -- not because people starve, but because they
  lose the will to continue.
- **How it applies**: Settlement morale should be a first-class visible system, not
  an afterthought. In terminal, a prominently placed morale bar or sparkline would
  communicate the settlement's emotional arc. Policy decisions (rationing, work
  hours, religious freedom) should have trade-offs that ripple through the social
  simulation. **Make the player feel like a leader, not an optimizer.**

### Manor Lords -- Organic Historical Growth

- **Source**: https://manorlords.com/ | https://en.wikipedia.org/wiki/Manor_Lords
- **What makes it exceptional**: Gridless placement inspired by real 14th-century
  Franconian town planning. The burgage plot system subdivides housing areas based
  on road proximity and available space. Buildings scale dynamically. Layout affects
  worker efficiency and defensive capability -- it's not cosmetic.
- **Brilliant design decision**: Historical accuracy as gameplay. The reason medieval
  towns look the way they do is because of practical constraints. By modeling those
  constraints, authentic-looking settlements emerge naturally.
- **How it applies**: Settlement layout in terminal should follow geographic logic.
  Buildings near water, roads along ridgelines, markets at crossroads -- not because
  we force it, but because the simulation rewards it. **Organic growth from
  incentives, not grid snapping.** Even in ASCII, a settlement that follows terrain
  contours reads as more real than a perfect grid.

### Foundation -- Gridless Zones

- **Source**: https://www.polymorph.games/en/ | https://store.steampowered.com/app/690830/Foundation/
- **What makes it exceptional**: You don't place buildings; you paint zones and
  residents self-organize. Modular monument construction: buildings are assembled
  from parts, so every player's town looks different. Three progression paths
  (Labor, Clergy, Kingdom) that can be mixed.
- **How it applies**: Zone painting is perfect for terminal. Select a region, assign
  a function (residential, farming, workshop), and watch the settlement fill it
  organically. This avoids the tedium of placing individual structures. The
  multi-path progression (economic, military, cultural) adds replayability.

### Going Medieval -- The Z-Axis Matters

- **Source**: https://foxyvoxel.io/games/going-medieval/ | https://store.steampowered.com/app/1029780/Going_Medieval/
- **What makes it exceptional**: 3D voxel terrain with underground construction.
  Digging underground creates storage but introduces vermin and flooding. Insulating
  layers for temperature control. The z-axis makes defense meaningful (walls,
  towers, moats).
- **How it applies**: terrain-gen-rust already has geology layers. Showing depth
  in terminal is the challenge. Layer-switching (press < > to move between z-levels)
  is the classic DF approach. Heat maps for underground temperature, water table
  indicators, and soil composition overlays make the z-axis legible in ASCII.

### Patron -- Individual Social Simulation

- **Source**: https://www.overseer-games.com/patron | https://store.steampowered.com/app/1538570/Patron/
- **What makes it exceptional**: Every citizen has individual opinions on immigration,
  taxes, religion, health, and safety. Social classes (Peasant, Laborer, Merchant,
  Gentry) have different needs. "Happiness is a complete gameplay system in its own
  right."
- **Underappreciated**: This game got mediocre reviews but its *social simulation
  granularity* is ahead of most colony sims. The mistake was making it visible but
  not actionable enough.
- **How it applies**: Individual opinion tracking makes emergent social dynamics
  possible. The key is surfacing it well. In terminal: a population panel showing
  aggregate satisfaction by category, with drill-down to individual citizens.
  Sparklines showing trends over time. **Citizens should feel like a populace, not
  a resource counter.**

---

## 2. Information Visualization in Terminal/ASCII

### Cogmind -- The Gold Standard of ASCII UI

- **Source**: https://www.gridsagegames.com/cogmind/innovation.html | https://www.gridsagegames.com/blog/2024/01/full-ui-upscaling-part-1-history-and-theory/
- **What makes it exceptional**: Kyzrati (Grid Sage Games) treats ASCII not as a
  limitation but as a design language. Key innovations:
  - **Mixed font dimensions**: Wide fonts for text readability, narrow fonts for map
    fidelity. No other roguelike does this.
  - **Drag-and-drop in ASCII**: First roguelike to support it. Every command is
    accessible via both keyboard and mouse.
  - **~1000 procedural particle effects** in ASCII. Explosions, sparks, energy
    fields -- all using CP437 characters and color.
  - **Automatic object labeling**: Items and entities get floating labels that
    auto-position to avoid overlap.
  - **Color-coded property labels**: Different item stats use different colors,
    letting you scan visually.
  - **Grid-based zooming**: Larger bitmap fonts spanning multiple cells for dynamic
    scaling without breaking the terminal aesthetic.
  - **Ambient distance-based sound**: Environmental objects emit sounds that change
    with distance and cease when destroyed.
- **The philosophy**: The interface IS the game. In ASCII, you can't separate UI from
  content -- the map, the HUD, the inventory are all rendered in the same medium.
  This means every pixel of screen real estate must earn its place.
- **How it applies**: This is the benchmark. terrain-gen-rust should aim for:
  mixed-width rendering (map vs. text panels), color-coded overlays that switch
  context (geology, population, economy, defense), and particle effects for weather
  and events (rain as falling characters, fire as animated glyphs). **Invest in the
  terminal renderer as much as the simulation.**

### Brogue -- Color as Atmosphere

- **Source**: https://sites.google.com/site/broguegame/ | https://waltoriouswritesaboutgames.com/2011/10/26/roguelike-highlights-brogue/
- **What makes it exceptional**: Brian Walker's masterpiece proves that terminal
  graphics can be *beautiful*. Key techniques:
  - **"Dancing" colors**: Tiles slightly vary color even when idle, giving the
    dungeon a sense of life. Water shimmers. Lava pulses.
  - **Light propagation**: Spells, fires, and objects cast colored light that
    tints nearby tiles. A torch doesn't just illuminate -- it makes nearby walls
    warm orange.
  - **Gas density visualization**: Toxic gas clouds use color intensity to convey
    concentration. Lighter = more dispersed. This communicates gameplay information
    (safe to walk through?) through color alone.
  - **Simulated terminal, not actual terminal**: Brogue renders to a grid but
    controls every color value. This gives "vastly greater color depth than
    terminals tend to allow."
- **How it applies**: This is the aspiration for atmosphere. Season transitions
  should shift the palette (autumn warmth, winter blue-gray). Water should shimmer.
  Fires should cast light. Even in 256-color mode, subtle per-tile color variation
  makes the world feel alive. **The terminal should breathe.**

### DCSS (Dungeon Crawl Stone Soup) -- Interface Evolution Through Iteration

- **Source**: https://crawl.develz.org/ | https://crawl.develz.org/download.htm
- **What makes it exceptional**: 20+ years of UI iteration by an open-source
  community. Key lessons:
  - **Arrow key navigation everywhere**: v0.28 unified menu navigation, making
    every screen keyboard-friendly.
  - **Stash search with travel**: Find an item, press enter, auto-path to it. The
    information system is linked to the action system.
  - **Progressive disclosure**: Mouse popups show details on hover, keyboard users
    get context menus. Two parallel interaction modes, both complete.
  - **Web playable**: The tile version runs in a browser, proving terminal-style
    games can be networked.
- **How it applies**: For settlement sim, the search-and-navigate pattern is
  essential. "Show me all idle workers" -> select one -> camera jumps to them.
  "Find all food stores" -> overlay highlights them. **Information retrieval and
  action should be one flow.**

### NetHack's Symbol System -- Semantic Density

- **What makes it exceptional**: Every ASCII character has meaning. `@` is you,
  `d` is dog, `D` is dragon. `:` is a lizard. The entire bestiary is encoded in
  the alphabet. Players learn to read the screen like a language.
- **The deeper lesson**: ASCII forces **semantic density**. Each character must
  carry maximum meaning. In graphical games, a sprite can be decorative. In ASCII,
  every glyph is information.
- **How it applies**: Define a consistent glyph vocabulary. `^` = mountain, `~` =
  water, `#` = wall, `+` = door, `*` = ore/resource. Color modifies meaning
  (green `~` = swamp, blue `~` = river, white `~` = rapids). **Build a visual
  language players internalize.**

---

## 3. Color Theory for Games

### Biome Distinction Through Temperature Palettes

The most effective approach for terrain visualization uses warm/cool color
temperature as the primary axis:

| Biome        | Temperature | Key Colors                  | Terminal Approach         |
|-------------|-------------|----------------------------|--------------------------|
| Desert      | Hot         | Yellows, oranges, tan      | Yellow fg, dark bg       |
| Savanna     | Warm        | Gold, olive, brown         | Dark yellow fg           |
| Forest      | Cool-warm   | Deep greens, brown         | Green fg, dark bg        |
| Tundra      | Cold        | White, pale blue, gray     | White/cyan fg            |
| Jungle      | Hot-humid   | Vivid greens, dark         | Bright green, dark bg    |
| Mountain    | Cold-dry    | Gray, white, dark blue     | White fg, blue accents   |
| Swamp       | Warm-wet    | Olive, murky green, brown  | Dark green/yellow mix    |
| Ocean       | Variable    | Blues, deep navy            | Blue spectrum             |

### Breath of the Wild's Color-as-Wayfinding

- **Source**: https://www.irondragondesign.com/design-of-the-wild/
- Each culture/region gets a distinct palette: Zora (cool blues, silver), Gorons
  (warm reds, orange, black), Gerudo (pastels, gold), Sheikah (earth tones,
  purple accents).
- **The principle**: Color should communicate *meaning* before *beauty*. A player
  should know what biome they're in from color alone, without reading labels.
- **Terminal application**: Each biome needs a dominant hue. Don't use full-spectrum
  color everywhere -- restrict each biome's palette and the transitions between
  them will read as meaningful boundaries.

### Colorblind-Friendly Design

- **Source**: https://davidmathlogic.com/colorblind/ | https://chrisfairfield.com/unlocking-colorblind-friendly-game-design/
- Core rules:
  - Never rely on red/green distinction alone (8% of males are red-green colorblind)
  - Blue is the safest color -- it's perceived consistently across all color vision types
  - Use **shape + color** redundancy: factions get unique glyphs AND colors
  - Perceptually uniform palettes (Viridis, Magma, Cividis) for heat maps
  - The 60-30-10 rule: 60% dominant color, 30% secondary, 10% accent
- **Terminal application**: Use the Viridis palette for temperature/density overlays.
  Always pair color with glyph variation. Offer a colorblind mode that shifts to
  blue-orange instead of red-green.

### 256-Color Terminal Palette Strategy

- **Source**: https://www.ditig.com/256-colors-cheat-sheet
- The 256-color palette: 16 system colors (unreliable, user-themed), 216 RGB colors
  (6x6x6 cube), 24 grayscale shades.
- **Strategy**: Skip system colors 0-15 entirely. Use the 216-color cube (16-231)
  for predictable rendering. The grayscale ramp (232-255) is perfect for stone,
  mountains, and UI chrome.
- **For biomes**: Pick 3-4 colors per biome from the 216 cube. Ensure they're
  spread across brightness levels so they remain distinct in grayscale too.

---

## 4. Sound Design for Ambient Simulation

### Dwarf Fortress SoundSense -- Log-Driven Audio

- **Source**: https://www.dwarffortresswiki.org/index.php/Utility:SoundSense | https://github.com/prixt/soundsense-rs
- **How it works**: SoundSense reads `gamelog.txt` in real-time. When log patterns
  match (combat, weather change, strange mood), it triggers contextual sounds.
  Multiple sounds per event are weighted randomly for variety. It's essentially
  a **state machine driven by text parsing**.
- **Brilliant aspect**: Total decoupling. The game doesn't know about sound. The
  sound engine doesn't modify the game. They communicate through a text log. This
  is elegant architecture.
- **How it applies**: terrain-gen-rust could emit structured events to stdout or a
  log file. A separate audio process (even a simple Rust binary using rodio) reads
  events and plays sounds. Events: `season_change:winter`, `combat:start`,
  `construction:complete`, `weather:rain`. **Sound as a subscriber to simulation
  events, not a coupled system.**

### Cogmind's Distance-Based Ambient Sound

- Environmental objects emit sounds that attenuate with distance and cease when
  destroyed. This creates a spatial soundscape that reinforces the map.
- **Terminal application**: Even without positional audio, we can use intensity.
  When the viewport is near a river, water sounds play louder. Near a forge,
  hammering. Near a battle, clash of weapons. Volume scales with viewport
  proximity to the sound source.

### Procedural Audio Principles

- **Seasons**: Spring (birdsong, gentle wind), Summer (insects, heat haze hum),
  Autumn (wind through leaves, distant geese), Winter (silence punctuated by
  cracking ice, howling wind)
- **Time of day**: Dawn (roosters, awakening), Day (work sounds, chatter), Dusk
  (settling, fires being lit), Night (crickets, owls, distant wolves)
- **Weather layers**: Rain (light patter -> heavy drumming), Thunder (random
  timing, distant -> close), Wind (constant low, gusting high), Snow (muffled
  silence)
- **Implementation**: Layer multiple ambient loops with crossfading. Each loop
  tagged with conditions (season, time, weather). The mixer blends based on
  current simulation state.

---

## 5. Emergent Storytelling

### Dwarf Fortress Legends Mode -- History as Content

- **Source**: https://www.taylorfrancis.com/chapters/edit/10.1201/9780429488337-15/emergent-narrative-dwarf-fortress-tarn-adams | https://themadwelshman.com/exploring-dwarf-fortress-legends-mode/
- **What makes it exceptional**: Every narrative is emergent. No embedded stories.
  Legends Mode documents the life events of every historical figure -- births,
  battles, art created, murders committed, civilizations founded. The world has
  *history* before the player arrives.
- **Design insight**: The simulation generates too many events to follow. Legends
  Mode's genius is providing **tools to browse history**, not forcing you to
  witness it. Search, filter, cross-reference.
- **How it applies**: Generate settlement history during worldgen. When the player
  starts, the region already has ruins, old roads, abandoned mines with stories
  attached. A `legends` command in the terminal lets players browse: "In year 47,
  the settlement of Thornfield was abandoned after the Great Flood." **History gives
  meaning to terrain features.**

### RimWorld -- The AI Storyteller Architecture

- **Source**: https://www.gamedeveloper.com/design/rimworld-dwarf-fortress-and-procedurally-generated-story-telling | https://gamewithyourbrain.com/blog/2017/2/6/rimworld
- **What makes it exceptional**: Three storytellers (Cassandra, Phoebe, Randy) aren't
  difficulty settings -- they're narrative pacing engines. Cassandra builds
  tension in arcs. Randy is chaos. Phoebe gives breathing room. The storyteller
  modulates event frequency and intensity based on colony wealth and mood.
- **Art descriptions**: This is underappreciated. When a colonist creates art, the
  game generates a description referencing actual events: "A sculpture depicting
  the death of Engie Reyes by squirrel on 5th of Jugust, 5502." These become
  artifacts of the emergent narrative.
- **The retelling insight**: "The pleasure of RimWorld comes from sharing experiences
  with others. Not the original telling by the game, but the retelling to other
  players." The game generates raw material for human storytelling.
- **How it applies**: Implement a storyteller that modulates event intensity. Quiet
  periods followed by crises. Art/crafting descriptions that reference actual
  settlement history. A settlement log that reads like a chronicle. In terminal,
  an `events` or `chronicle` view that shows the narrative arc. **Give players
  stories worth retelling.**

### Crusader Kings -- Emergence Detection

- **Source**: https://gdcvault.com/play/1020774/Emergent-Stories-in-Crusader-Kings | https://killscreen.com/articles/fascinating-story-ai-behind-crusader-kings-2s-dark-chain-events/
- **What makes it exceptional**: Henrik Fahraeus (Paradox) articulated the recipe:
  "Open-ended gameplay, a great many AI actors, AI personalities and opinions,
  changing conditions, conflict, and low morals. Sprinkle with scripted narrative."
- **The emergence detection concept**: Paradox planned (and partially implemented)
  a system where the AI scans for dramatic patterns and *amplifies* them. Sad
  situation? Play sad music. Interesting conflict brewing? Nudge the RNG toward
  more interesting outcomes.
- **How it applies**: This is the most transferable insight. Don't just simulate --
  **detect interesting moments and highlight them**. When two settlement leaders
  have opposing opinions and a crisis hits, flag it. When a citizen's journey
  from refugee to master craftsman completes, note it. Terminal notifications
  like: "Mira, who arrived starving three winters ago, has become the settlement's
  finest blacksmith." **The simulation writes the story; the detection system
  edits it.**

### Caves of Qud -- Procedural Culture and Mythology

- **Source**: https://www.gamedeveloper.com/design/tapping-into-the-potential-of-procedural-generation-in-caves-of-qud | https://www.freeholdgames.com/papers/Generation_of_mythic_biographies_in_Cavesofqud.pdf
- **What makes it exceptional**: Generates village histories, cultures, architectural
  styles, storytelling traditions, and myths. The Sultan system creates historical
  figures with procedural biographies that include *bias and conflicting accounts*.
  Different NPCs tell different versions of the same history.
- **Underappreciated brilliance**: The procedural *unreliable narrator*. History
  isn't objective data -- it's stories told by people with agendas. This makes
  exploration feel like archaeology.
- **How it applies**: Settlement legends and oral histories. When the player asks
  about an old ruin, different citizens give different accounts. A scholar's
  version vs. a farmer's superstition. **Procedural history with unreliable
  narrators turns data into narrative.**

---

## 6. Novel UI Patterns for Complex Sims

### Overlay Modes (The ONI/Cities Skylines Pattern)

- Oxygen Not Included, Cities: Skylines, and Dwarf Fortress all use **toggled
  overlay views** that recolor the map to show one data dimension at a time:
  temperature, water pressure, happiness, traffic, etc.
- **Why it works**: Complex sims have too many data layers for simultaneous display.
  Overlays let the player focus on one concern at a time while maintaining spatial
  context.
- **Terminal implementation**: Mode keys (F1-F8 or similar):
  - Default: terrain + buildings + agents
  - F1: Temperature (Viridis palette heat map)
  - F2: Water/moisture (blue intensity gradient)
  - F3: Population density (warm colors for crowded)
  - F4: Happiness/morale (green = content, red = angry)
  - F5: Resource availability (color by type)
  - F6: Defense/threat (red zones for danger)
  - F7: Pathing/traffic (brighter = more traveled)
  - F8: Ownership/territory (faction colors)

### Sparklines in UI Panels

- **Source**: https://www.edwardtufte.com/notebook/sparkline-theory-and-practice-edward-tufte/
- Tufte's sparklines: "small, intense, simple, word-sized graphics." They show
  trends inline with text.
- **In settlement sim**: Next to "Food: 847" show a tiny 20-character sparkline
  of food levels over the last 30 days: `Food: 847 __|^^|__/~~\___`
  (using Unicode block characters or ASCII). Instantly communicates: "food is
  okay now but was scarce recently and trending down."
- **Terminal implementation**: Use Unicode block elements (U+2581 through U+2588,
  the eight levels of block fill) to create mini bar charts: `_...__---^^^---__`
  This gives 8 vertical levels in a single character height. Stunning data density.

### Factorio's Production Statistics

- **Source**: https://lua-api.factorio.com/latest/ | https://forums.factorio.com/viewtopic.php?t=10041
- Factorio tracks production and consumption rates for every item and displays
  them as time-series graphs. Players obsess over these.
- **Sankey diagrams** in the community visualize resource flow: ore -> plates ->
  circuits -> science. The width of the flow represents throughput.
- **How it applies**: A `stats` panel showing production/consumption rates for
  key resources. ASCII Sankey diagrams showing resource chains:
  ```
  Wheat ====> Flour ==> Bread
       \                  |
        `-> Animal Feed   v
              |       Tavern
              v
            Leather => Armor
  ```
  This can be generated from the actual simulation data.

### Radial/Context Menus

- Several modern games use radial menus for quick action selection. In terminal,
  the equivalent is **context-sensitive action lists**. Select a building, get
  only the actions relevant to that building type. Select a citizen, see their
  specific capabilities.
- **The principle**: Reduce cognitive load by showing only relevant options.

### The Minimap Problem

- In graphical colony sims, the minimap provides global context. In terminal,
  the "minimap" can be a condensed view using half-block characters or Braille
  patterns (U+2800 range), achieving 2x4 pixel resolution per character cell.
  A 40x20 character minimap = 80x80 effective pixels, enough to show settlement
  layout, terrain features, and threat indicators.

---

## 7. Modding Systems

### Factorio's Lua Architecture -- The Three-Phase Load

- **Source**: https://lua-api.factorio.com/latest/auxiliary/data-lifecycle.html | https://wiki.factorio.com/Tutorial:Modding_tutorial/Gangsir
- **Architecture**: Three load phases per mod:
  1. `data.lua` -- define new prototypes
  2. `data-updates.lua` -- modify other mods' prototypes
  3. `data-final-fixes.lua` -- last-pass overrides
- All mods share a single Lua state per phase. This three-phase approach lets
  mods cooperate without explicit dependencies.
- **Hot reload**: `control.lua` (runtime logic) reloads on save/load. Prototype
  changes require full mod reload via `game.reload_mods()`.
- **Why it works**: Separating data definition from runtime behavior. Data is
  declarative (what exists), control is imperative (what happens).
- **How it applies**: For terrain-gen-rust, a similar architecture:
  1. Data phase: Lua/JSON files define biomes, resources, buildings, recipes
  2. Logic phase: Lua scripts define AI behaviors, event handlers
  3. Hot reload: Logic scripts can be reloaded without restarting. Data changes
     require a world reload.
  This matches the existing Lua AI idea in the project memory.

### Noita's Pixel Physics + Lua

- **Source**: https://noita.wiki.gg/wiki/Modding:_Lua_API | https://noita.fandom.com/wiki/Modding:_Lua_Scripting
- Noita's world is 512x512 chunks of individually simulated pixels. The C++
  engine handles physics; Lua handles game logic, entity behavior, and modding.
- **The lesson**: Simulation core in a fast language (C++/Rust), behavior scripting
  in a flexible one (Lua). The boundary is the key design decision: what crosses
  the Rust/Lua barrier?
- **How it applies**: Terrain generation, pathfinding, and physics stay in Rust.
  Building definitions, AI decision trees, event responses, and quest logic go
  in Lua. The Rust engine exposes a clean API: `get_tile(x,y)`,
  `spawn_entity(type,x,y)`, `get_nearest(type,x,y,radius)`.

### Cataclysm: DDA -- JSON as the Universal Language

- **Source**: https://docs.cataclysmdda.org/MODDING.html | https://github.com/CleverRaven/Cataclysm-DDA/blob/master/doc/MODDING.md
- **What makes it exceptional**: The base game itself is defined in JSON. Items,
  monsters, recipes, terrain -- everything is an external data file. Mods are
  just more JSON in a different folder. There's no distinction between "core
  content" and "mod content" at the engine level.
- **Why it matters**: This is the most accessible modding system possible. No
  programming knowledge needed to add items, change recipes, or create new
  buildings. Just edit a JSON file.
- **How it applies**: For terrain-gen-rust, building types, resource definitions,
  biome parameters, and recipe chains should all live in JSON/TOML files that
  the engine loads at startup. The Lua scripting layer handles behavior; the
  data files handle content. This two-tier approach (JSON for data, Lua for
  logic) maximizes moddability.

### Dwarf Fortress Hack (DFHack) -- The Plugin Architecture

- DFHack is an external tool that hooks into DF's memory and adds functionality.
  It demonstrates the value of exposing simulation internals. SoundSense,
  Armok Vision, Dwarf Therapist -- the entire DF tool ecosystem exists because
  the simulation state is readable.
- **How it applies**: Expose simulation state via a structured interface (TCP
  socket, shared memory, or simply a well-formatted log). This enables external
  tools: visualizers, sound engines, AI trainers, analytics dashboards. **The
  simulation should be observable, not just playable.**

---

## 8. Synthesis: Principles for terrain-gen-rust

Drawing from all of the above, these are the highest-value principles:

### Architecture
1. **Simulation-first, rendering-second**: The terminal is a dashboard for a
   rich simulation. Invest 80% in simulation fidelity, 20% in rendering.
2. **Layered overlays**: 8+ toggle-able views of the same map data. Each overlay
   uses a consistent, colorblind-friendly palette.
3. **Event-driven architecture**: Simulation emits structured events. Sound,
   UI notifications, chronicle entries, and mod hooks all subscribe to events.
4. **Two-tier moddability**: JSON/TOML for content (data), Lua for behavior
   (logic). Hot-reload for Lua scripts.

### Visual Design
5. **Brogue-style atmosphere**: Dancing colors for water/fire. Light propagation
   from heat sources. Seasonal palette shifts.
6. **Cogmind-level UI craft**: Mixed font widths. Color-coded information.
   Every screen element earns its place.
7. **Sparklines everywhere**: Inline trend indicators next to every numeric
   stat. Use Unicode block elements for 8-level resolution per character.
8. **Consistent glyph vocabulary**: Document the full symbol set. Color
   modifies meaning within a type (green `~` vs blue `~`).

### Gameplay
9. **ONI layered challenges**: No permanently solved problems. Systems
   interconnect so that solving one creates pressure elsewhere.
10. **Frostpunk moral weight**: Policy decisions with real trade-offs visible
    in morale/productivity metrics. Make the player feel consequences.
11. **CK3 emergence detection**: Scan for interesting narrative patterns and
    surface them. Don't just simulate -- curate the story.
12. **Caves of Qud unreliable history**: Generated lore with multiple
    perspectives. Exploration reveals layers of history.

### Social Simulation
13. **Patron-style individual opinions**: Citizens have views on specific
    issues, not just a single happiness number.
14. **Songs of Syx scale through automation**: Zone-based control, not
    individual micro-management. Let the AI handle the mundane.
15. **RimWorld storyteller pacing**: Modulate crisis frequency and intensity.
    Quiet periods make crises feel dramatic.

---

## Appendix: Underappreciated Games Worth Studying

| Game | Why It's Underappreciated | Key Insight |
|------|--------------------------|-------------|
| **Caves of Qud** | Dismissed as "just another roguelike" | Procedural culture generation is a decade ahead of everything else |
| **Patron** | Mediocre reviews, good systems | Per-citizen opinion modeling on political issues |
| **Foundation** | "Pretty but shallow" complaints | Zone painting + organic growth is the right UX for settlement games |
| **Songs of Syx** | Solo dev, pixel art stigma | Proves 50K-agent sim is achievable; automation-first design |
| **Brogue** | "Just a simple roguelike" | The most beautiful terminal game ever made; color theory masterclass |
| **Cogmind** | Niche audience | ASCII UI design that rivals professional GUI applications |
| **Cataclysm: DDA** | Overwhelming complexity | JSON-as-engine proves data-driven design works at massive scale |

---

## Sources

### Colony Sims
- [Songs of Syx Official](https://songsofsyx.com/)
- [Songs of Syx Review -- Reality Remake](https://www.realityremake.com/articles/songs-of-syx-review-a-brutally-complex-colony-sim-with-endless-replay)
- [Songs of Syx -- PC Gamer](https://www.pcgamer.com/songs-of-syx-is-a-base-building-game-with-massive-scale-battles/)
- [ONI Design -- Gamedeveloper](https://www.gamedeveloper.com/design/behind-the-design-of-hit-sim-game-i-oxygen-not-included-i-)
- [ONI Layering Challenges -- Gamedeveloper](https://www.gamedeveloper.com/design/layering-challenges-in-klei-s-survival-sim-i-oxygen-not-included-i-)
- [Frostpunk Moral Choices -- Medium](https://brickwallpictures.medium.com/how-frostpunk-injects-harrowing-moral-choices-into-the-city-builder-genre-8536ad222cee)
- [Frostpunk Scenario Writing -- IGC 2018](https://www.invenglobal.com/articles/6528/igc-2018-empathize-with-numbers-the-process-of-frostpunk-scenario-writing)
- [Frostpunk Game Design -- Retro Style Games](https://retrostylegames.com/blog/frostpunk-game-design/)
- [Manor Lords Official](https://manorlords.com/)
- [Manor Lords -- Wikipedia](https://en.wikipedia.org/wiki/Manor_Lords)
- [Foundation -- Polymorph Games](https://www.polymorph.games/en/)
- [Foundation -- Steam](https://store.steampowered.com/app/690830/Foundation/)
- [Going Medieval -- Foxy Voxel](https://foxyvoxel.io/games/going-medieval/)
- [Going Medieval -- Steam](https://store.steampowered.com/app/1029780/Going_Medieval/)
- [Patron -- Overseer Games](https://www.overseer-games.com/patron)
- [Patron -- Steam](https://store.steampowered.com/app/1538570/Patron/)

### ASCII/Terminal UI
- [Cogmind Innovation Page](https://www.gridsagegames.com/cogmind/innovation.html)
- [Cogmind UI Upscaling Blog](https://www.gridsagegames.com/blog/2024/01/full-ui-upscaling-part-1-history-and-theory/)
- [Cogmind Roguelike Blog](https://www.gridsagegames.com/blog/2015/04/cogmind-roguelike/)
- [Brogue Official](https://sites.google.com/site/broguegame/)
- [Brogue Highlights -- Waltorius](https://waltoriouswritesaboutgames.com/2011/10/26/roguelike-highlights-brogue/)
- [DCSS Official](https://crawl.develz.org/)
- [REXPaint -- Gamedeveloper](https://www.gamedeveloper.com/design/roguelike-development-with-rexpaint)

### Color Theory
- [Design of the Wild -- Iron Dragon Design](https://www.irondragondesign.com/design-of-the-wild/)
- [Colorblind Design -- David Nichols](https://davidmathlogic.com/colorblind/)
- [Colorblind Game Design -- Chris Fairfield](https://chrisfairfield.com/unlocking-colorblind-friendly-game-design/)
- [256 Color Cheat Sheet](https://www.ditig.com/256-colors-cheat-sheet)
- [Colorblind Palettes -- Venngage](https://venngage.com/blog/color-blind-friendly-palette/)

### Sound Design
- [SoundSense -- DF Wiki](https://www.dwarffortresswiki.org/index.php/Utility:SoundSense)
- [soundsense-rs -- GitHub](https://github.com/prixt/soundsense-rs)

### Emergent Storytelling
- [Emergent Narrative in DF -- Taylor & Francis](https://www.taylorfrancis.com/chapters/edit/10.1201/9780429488337-15/emergent-narrative-dwarf-fortress-tarn-adams)
- [DF Legends Mode -- The Mad Welshman](https://themadwelshman.com/exploring-dwarf-fortress-legends-mode/)
- [RimWorld Storytelling -- Gamedeveloper](https://www.gamedeveloper.com/design/rimworld-dwarf-fortress-and-procedurally-generated-story-telling)
- [RimWorld Story Analysis -- Game With Your Brain](https://gamewithyourbrain.com/blog/2017/2/6/rimworld)
- [CK2 Emergent Stories -- GDC Vault](https://gdcvault.com/play/1020774/Emergent-Stories-in-Crusader-Kings)
- [CK2 Story AI -- Kill Screen](https://killscreen.com/articles/fascinating-story-ai-behind-crusader-kings-2s-dark-chain-events/)
- [Caves of Qud Procedural Generation -- Gamedeveloper](https://www.gamedeveloper.com/design/tapping-into-the-potential-of-procedural-generation-in-caves-of-qud)
- [Caves of Qud Mythic Biographies -- Freehold Games](https://www.freeholdgames.com/papers/Generation_of_mythic_biographies_in_Cavesofqud.pdf)

### UI Patterns
- [Factorio Production Visualization -- Forum](https://forums.factorio.com/viewtopic.php?t=10041)
- [Factorio Recipe Visualization -- Kevin Ta](https://kevinta.ca/10/12/2018/Factorio-Recipe-Visualization/)
- [Sparkline Theory -- Edward Tufte](https://www.edwardtufte.com/notebook/sparkline-theory-and-practice-edward-tufte/)
- [Game UI Database](https://www.gameuidatabase.com/)

### Modding Systems
- [Factorio Data Lifecycle](https://lua-api.factorio.com/latest/auxiliary/data-lifecycle.html)
- [Factorio Modding Tutorial](https://wiki.factorio.com/Tutorial:Modding_tutorial/Gangsir)
- [Noita Lua API](https://noita.wiki.gg/wiki/Modding:_Lua_API)
- [CDDA Modding Guide](https://docs.cataclysmdda.org/MODDING.html)
- [CDDA JSON Format -- GitHub](https://github.com/CleverRaven/Cataclysm-DDA/blob/master/doc/MODDING.md)
- [Armok Vision -- GitHub](https://github.com/RosaryMala/armok-vision)
