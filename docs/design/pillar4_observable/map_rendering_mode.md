# Map Rendering Mode (Symbolic ASCII)

*Design doc for Pillar 4, Mode A*
*Last updated: 2026-04-01*

## Problem

The current renderer tries to be both atmospheric and readable, and falls into an uncanny valley between the two. Vegetation characters (`♠♣"`) fight with terrain characters (`':.;`) for visual attention. Lighting darkens everything at night, making entities hard to spot. Water shimmer animation is nice but adds noise when you're trying to parse the map. The result: you can't quickly answer "where are my villagers and what are they doing?"

Map Mode fixes this by committing fully to one philosophy: **characters carry ALL the meaning, color is flat and minimal.** Every tile is instantly parseable. Entities pop because they are unique glyphs on a clean, low-contrast background.

## Design Philosophy

**Dwarf Fortress energy.** The map is a symbolic diagram, not a painting. You read it like a blueprint. Each character is a word in a visual language. Once you learn the vocabulary (5 minutes), you can parse a 200x60 viewport in a glance.

Core rules:
1. **One glyph = one meaning.** No character is reused across categories (terrain, entity, building). If `~` is water, nothing else is `~`.
2. **Color = biome/category, not lighting.** Green means fertile. Blue means water. Brown means dry. Grey means stone. No gradients, no shading, no day/night dimming.
3. **Entities are bright on muted terrain.** Terrain uses dim, desaturated colors. Entities use vivid, saturated colors. The contrast makes agents visible without any overlay.
4. **Behavior is visible from the glyph.** A villager carrying wood looks different from a villager farming. You don't need the task overlay to know what's happening.
5. **No animation on terrain.** Water doesn't shimmer. Vegetation doesn't sway. The map is stable. Only entities move. Your eye tracks motion = your eye tracks agents.

## Terrain Glyph Table

Every terrain type gets exactly one character and one flat color pair. No lighting applied.

| Terrain       | Glyph | Foreground        | Background        | Rationale |
|---------------|-------|-------------------|-------------------|-----------|
| Grass         | `.`   | `(60, 140, 60)`   | `(30, 80, 30)`    | Low visual weight. The "empty space" of the map. Dot is the quietest character. |
| Forest        | `♠`   | `(20, 100, 25)`   | `(15, 60, 18)`    | Distinct silhouette. Reads as tree canopy. Dense areas look like forest. |
| Water         | `~`   | `(70, 120, 220)`  | `(20, 40, 110)`   | Universal water symbol. Static (no animation in map mode). |
| Sand          | `,`   | `(190, 170, 100)` | `(150, 135, 80)`  | Comma = sparse, gritty. Low visual weight like grass but warm-toned. |
| Desert        | `:`   | `(200, 180, 120)` | `(160, 140, 90)`  | Two dots = dry, cracked. Slightly more visual weight than sand. |
| Mountain      | `▲`   | `(140, 130, 120)` | `(90, 82, 75)`    | Iconic peak symbol. Reads as elevation at any density. |
| Snow          | `*`   | `(220, 225, 240)` | `(180, 185, 200)` | Asterisk = snowflake. Bright but low-saturation. |
| Cliff         | `#`   | `(110, 100, 85)`  | `(65, 60, 50)`    | Hash = impassable wall. Heavy visual weight signals "can't walk here." |
| Marsh         | `"`   | `(50, 110, 70)`   | `(30, 65, 45)`    | Quote marks = reeds poking up from muck. Green-tinted. |
| Tundra        | `-`   | `(155, 165, 175)` | `(120, 130, 140)` | Dash = flat, barren, frozen. Cool grey-blue. |
| Scrubland     | `;`   | `(140, 125, 65)`  | `(100, 90, 45)`   | Semicolon = sparse brush. Warm, dry tone. |
| Road          | `=`   | `(170, 145, 90)`  | `(120, 100, 60)`  | Double line = paved path. Warm brown, high contrast with terrain. |
| BuildingFloor | `+`   | `(150, 130, 100)` | `(100, 85, 65)`   | Plus = constructed interior. Distinct from all terrain. |
| BuildingWall  | `#`   | `(170, 150, 120)` | `(120, 105, 85)`  | Hash = solid wall. Same glyph as cliff (both impassable) but warmer color distinguishes. |

### Vegetation overlay (replaces current `♠♣"` system)

In Map Mode, the vegetation simulation layer modifies the base terrain glyph only on Grass and Scrubland tiles. Forest tiles always show `♠`.

| Vegetation density | Glyph override | Color override         |
|--------------------|----------------|------------------------|
| 0.0 - 0.2          | (none, show base terrain) | (none)          |
| 0.2 - 0.5          | `'`            | `(50, 150, 50)` fg     |
| 0.5 - 0.8          | `♣`            | `(25, 120, 30)` fg     |
| 0.8 - 1.0          | `♠`            | `(15, 90, 20)` fg      |

This means a grassland with moderate vegetation shows `♣` in green, while bare grassland shows `.` -- you can read deforestation at a glance.

### Fog of war

Unrevealed tiles render as a single character with minimal color:

| State       | Glyph | Foreground       | Background       |
|-------------|-------|------------------|------------------|
| Unexplored  | `░`   | `(35, 35, 40)`   | `(12, 12, 15)`   |

No change from current behavior. The low contrast makes unexplored areas recede visually.

## Entity Glyph Table

Entities use bright, saturated colors on transparent backgrounds (terrain shows through). Each species/type has a unique glyph that never appears in terrain.

### Creatures

| Entity    | Glyph | Foreground         | Notes |
|-----------|-------|--------------------|-------|
| Villager  | `@`   | `(80, 200, 255)`   | The classic roguelike "you" symbol. Bright cyan pops against all terrain. |
| Wolf      | `w`   | `(200, 50, 50)`    | Lowercase = animal. Red = danger. Instantly readable. |
| Rabbit    | `r`   | `(190, 155, 90)`   | Lowercase = animal. Warm brown, not alarming. |

### Villager behavior glyphs

This is the key innovation of Map Mode. The villager glyph changes based on what they're doing, so you can read settlement activity without any overlay.

| Behavior State      | Glyph | Foreground         | Rationale |
|---------------------|-------|--------------------|-----------|
| Idle / Wander       | `@`   | `(80, 200, 255)`   | Default. Cyan. "Available." |
| Seeking (any)       | `@`   | `(180, 200, 100)`  | Same glyph, yellow-green tint. "Moving with purpose." |
| Gathering wood      | `$`   | `(160, 110, 50)`   | Dollar = resource extraction. Brown = wood. |
| Gathering stone     | `$`   | `(150, 150, 160)`  | Dollar = resource extraction. Grey = stone. |
| Gathering food      | `$`   | `(80, 200, 80)`    | Dollar = resource extraction. Green = food. |
| Hauling             | `%`   | `(220, 190, 60)`   | Percent = carrying load. Gold = productive. |
| Building            | `&`   | `(255, 220, 50)`   | Ampersand = construction. Bright yellow. |
| Farming             | `~`   | N/A -- see below   | Tilde would conflict with water. Use `f` instead. |
| Farming             | `f`   | `(80, 200, 80)`    | Lowercase = at-work activity. Green = agriculture. |
| Working (workshop)  | `g`   | `(210, 140, 60)`   | Lowercase = at-work activity. Orange = industry. |
| Sleeping            | `z`   | `(100, 100, 180)`  | Universal sleep symbol. Dim blue. |
| Fleeing             | `!`   | `(255, 60, 60)`    | Exclamation = alarm. Bright red. Unmissable. |
| Exploring           | `?`   | `(160, 220, 160)`  | Question mark = "what's out there?" Light green. |
| Captured            | `x`   | `(120, 30, 30)`    | Lowercase x = downed. Dark red. |

### Wolf behavior glyphs

| Behavior State | Glyph | Foreground         |
|----------------|-------|--------------------|
| Wander         | `w`   | `(200, 50, 50)`    |
| Hunting        | `W`   | `(255, 40, 40)`    | Uppercase = actively aggressive. Brighter red. |

### Rabbit behavior glyphs

| Behavior State | Glyph | Foreground         |
|----------------|-------|--------------------|
| Wander         | `r`   | `(190, 155, 90)`   |
| Eating         | `r`   | `(140, 200, 90)`   | Greener tint while eating. |
| Fleeing        | `!`   | `(255, 150, 50)`   | Orange `!` (not red -- prey alarm is less critical than villager alarm). |
| AtHome         | (hidden) | N/A             | Not rendered, same as current behavior. |

## Building Glyph Table

Buildings are tile-based (they overwrite terrain tiles). But in Map Mode, completed buildings also place a **marker glyph** at their center tile to identify type. The surrounding wall/floor tiles use the terrain glyphs above (`#` for wall, `+` for floor).

| Building    | Center glyph | Foreground         | Notes |
|-------------|-------------|--------------------|-------|
| Hut         | `⌂`         | `(170, 140, 100)`  | Classic house symbol. Warm wood color. |
| Stockpile   | `■`         | `(190, 150, 60)`   | Solid square = storage. Gold-brown. |
| Farm        | `≡`         | `(90, 180, 60)`    | Triple bar = tilled rows. Green. |
| Workshop    | `⚙`         | `(200, 180, 110)`  | Gear = industry. Warm metal. |
| Smithy      | `∆`         | `(200, 100, 40)`   | Triangle = anvil/furnace. Orange-hot. |
| Garrison    | `⚔`         | `(180, 50, 50)`    | Crossed swords = military. Red. |
| Granary     | `G`         | `(200, 180, 80)`   | Letter G = grain storage. Wheat-gold. |
| Bakery      | `B`         | `(210, 160, 90)`   | Letter B = bakery. Warm bread color. |
| Town Hall   | `H`         | `(255, 220, 60)`   | Letter H = hall. Bright gold = prestige. |
| Wall (1x1)  | `#`         | `(170, 150, 120)`  | Same as BuildingWall terrain. |
| Road (1x1)  | `=`         | `(170, 145, 90)`   | Same as Road terrain. |
| Build site  | `?`         | `(200, 180, 100)`  | Question mark in construction yellow. "Something is being built here." |

Note: Build site `?` conflicts with explorer `?`. This is acceptable because build sites are stationary and explorers move. If ambiguity is a problem in practice, build sites can use `_` (underscore = foundation) instead.

### Resource deposits and world objects

| Object         | Glyph | Foreground         | Notes |
|----------------|-------|--------------------|-------|
| Berry bush     | `♦`   | `(210, 50, 90)`    | Diamond = valuable food source. Red-pink. |
| Stone deposit  | `●`   | `(160, 155, 145)`  | Filled circle = mineral vein. Grey. |
| Den (rabbit)   | `O`   | `(140, 105, 65)`   | Open circle = burrow entrance. Brown. |

## Color Palette Philosophy

Map Mode uses a **limited, hand-picked palette** organized by semantic category:

| Category        | Hue family       | Saturation | Brightness |
|-----------------|------------------|------------|------------|
| Fertile terrain | Green            | Low        | Medium     |
| Dry terrain     | Yellow-brown     | Low        | Medium     |
| Water           | Blue             | Medium     | Medium     |
| Stone/mountain  | Grey             | Minimal    | Low-medium |
| Cold terrain    | Blue-grey        | Minimal    | High       |
| Villagers       | Cyan             | High       | High       |
| Threats         | Red              | High       | High       |
| Wildlife        | Brown/orange     | Medium     | Medium     |
| Buildings       | Warm neutrals    | Medium     | Medium     |
| Resources       | Category-colored | High       | Medium     |
| Construction    | Yellow           | High       | High       |
| Alerts          | Red/orange       | Maximum    | Maximum    |

The key constraint: **terrain is always dimmer and less saturated than entities.** This creates a natural visual hierarchy where agents and buildings are figure, terrain is ground.

## No Lighting, No Seasons (in Map Mode)

Map Mode does NOT apply:
- Day/night lighting tint
- Seasonal color shifts
- Water shimmer animation
- Weather particle effects

The map looks identical at noon and midnight. The panel still shows time/season. This is deliberate: Map Mode is for **gameplay decisions**, and those shouldn't depend on squinting through a blizzard. Atmosphere is Landscape Mode's job.

Exception: **fog of war** still applies. Unexplored areas are dark. This is gameplay-relevant information, not atmosphere.

## Example: Early Settlement (tick ~2000)

```
                    Map Mode — Early Settlement
 TERRAIN-GEN     ........♠♠♠♠♠♠.......~~~...........
 ─────────────   .......♠♠♠♠♠♠♠♠.....~~~...........
 Spring, Y1 D12  ......♠♠♠♠$♠♠♠♠.....~~~....,,,,..
 Mild            .....♠♠♠♠♠.♠♠♠......~~~...,,,,,,..
                 .......♠♠..........%~~~.....,,,,..
 Pop: 8  W: 0   .........♦..........~~~~..........
                 ........♦..@.......~~~~..........
 Resources       ...........?...###+###...▲▲▲▲▲..
  Food:  12      ..........+■+..#+⌂+#...▲▲●▲▲▲..
  Wood:  24      ..........+■+..###+###..▲▲▲$▲▲..
  Stone: 8       ...........≡≡≡.........▲▲▲▲▲▲▲..
                 ...........≡f≡.........▲▲▲▲▲▲▲..
 Population      ...........≡≡≡....z.....▲▲▲▲▲▲..
  Villagers: 8   ........................▲▲▲▲▲▲..
  Rabbits:  4    ..........O..r.............▲▲▲..
  Wolves:   0    ............r...............▲▲..
```

What you can read at a glance:
- **Top-left**: dense forest (`♠♠♠♠`). One villager gathering wood inside it (`$` in brown).
- **Center**: the settlement core. Stockpile (`■` in gold) flanked by floor tiles (`+`). A hut (`⌂`) just north. Build site foundation visible.
- **South of stockpile**: a 3x3 farm (`≡`) with a farmer working it (`f` in green).
- **Right side**: mountain range (`▲▲▲`) with a stone deposit (`●`). A villager mining stone inside the mountains (`$` in grey).
- **River**: runs north-south (`~~~`). A villager hauling resources crosses near it (`%` in gold).
- **One idle villager** (`@` in cyan) near the stockpile. One sleeping (`z` in dim blue) south of the mountains.
- **Two rabbits** (`r`) near their den (`O`) in the south. No wolves on screen.
- **Berry bushes** (`♦`) west of the settlement, partially foraged.

## Example: Established Settlement (tick ~15000)

```
                    Map Mode — Established Settlement
 TERRAIN-GEN     ░░░░░░░░░..'♣♠♠♠♠♠♠♠♠♠~~~.......
 ─────────────   ░░░░░░░..'..'♠♠$♠♠♠♠♠♠~~~.......
 Autumn, Y3 D5   ░░░░░░.........♠♠♠♠♠♠.~~~..▲▲▲..
 Cool  [3x]      ░░░░░....===========..~~~.▲▲●▲▲..
                 ░░░░.....=..........=.~~~.▲▲$▲▲..
 Pop: 32  W: 3  ░░░......=..###+###.=.~~~.▲▲▲▲▲..
                 ░░......=..#+⌂+#..=..~~~~.▲▲▲▲..
 Resources       ░.......=..###+###.=.~~~~..▲▲▲..
  Food:  85      ........=..+■++■+..=..~~~........
  Wood:  41      ........=..+■++■+..=..~~~........
  Stone: 23      ........=..........=..~~~........
  Planks:  7     ........=.###+###..=...~~........
  Masonry: 3     ........=.#+⚙+#%..=.............
  Grain:  12     .≡≡≡≡≡..=.###+###..=.............
  Bread:   4     .≡f≡f≡..=..........=.............
                 .≡≡≡≡≡..============.............
 Population      .≡≡f≡≡.....⚔.@.@.........w......
  Villagers: 32  .≡≡≡≡≡..≡≡≡≡≡..###+###..........
  Rabbits:  6    .........≡≡g≡≡..#+G+#.!..........
  Wolves:   3    .........≡≡≡≡≡..###+###..w.......
                 ..............z.z.z......w........
```

What you can read at a glance:
- **Northwest**: fog of war (`░`) -- unexplored frontier. Exploration is incomplete.
- **Road network** (`=`): a loop road connecting the forest, the settlement core, and the mining area. Emerged from traffic.
- **Two stockpiles** (`■■`) in the center, flanked by buildings. A hut (`⌂`) north of them.
- **Workshop** (`⚙`) south of stockpiles with a hauler (`%`) bringing materials.
- **Two large farm complexes** with farmers at work (`f`). One farmer at a processing building (`g` in orange) -- the granary or bakery.
- **Garrison** (`⚔`) guarding the south approach. Two idle villagers (`@`) nearby as reserve.
- **Granary** (`G`) to the southeast, recently completed.
- **Three wolves** (`w`) circling the eastern perimeter. One villager fleeing (`!` in red) -- this is the most urgent thing on screen and it reads instantly.
- **Sleeping villagers** (`zzz`) in the south, night shift resting.
- **Mining operation**: stone deposit (`●`) in the mountains with an active miner (`$`).
- **Deforested area**: the forest northeast has gaps (`.` instead of `♠`) where wood was harvested.

## Example: Wolf Attack in Progress

```
  ...♠♠♠♠♠...=====.▲▲▲▲▲▲.
  ...♠♠♠♠....=.@.=.▲▲▲▲▲▲.
  ...♠♠♠.....=...=.▲▲●▲▲▲.
  ............=...=..▲▲▲▲▲.
  ....♦...###+###.=..▲▲▲▲..
  .........#+⌂+#..=.........
  .........###+###.=.........
  ..........+■++■+.=.........
  ......≡≡≡.+■++■+.=.........
  ......≡f≡.........=........
  ......≡≡≡..⚔.!...W.w......
  ...........@.@.!.W.........
  ................!..w........
  ..........z.z.z............
```

The crisis is instantly readable:
- **Three `W` wolves** (uppercase = hunting) closing in from the east.
- **Three `!` villagers** fleeing toward the garrison (`⚔`).
- **Two `@` villagers** near the garrison, possibly about to flee too.
- **One `f` farmer** still working, unaware (out of sight range).
- **Sleeping villagers** (`z`) in the south, vulnerable.
- The garrison (`⚔`) is the defensive anchor. Are there enough villagers to hold?

## Implementation Plan

### Phase 1: Core glyph swap (Mode A foundation)

Files to modify:
- `src/tilemap.rs` -- Add `map_ch()`, `map_fg()`, `map_bg()` methods to `Terrain` enum (parallel to existing `ch()`, `fg()`, `bg()` which become Landscape Mode methods).
- `src/game/render.rs` -- Add `draw_map_mode()` method that renders terrain using `map_*()` methods, skips all lighting/season/water-animation logic.
- `src/game/mod.rs` -- Add `RenderMode::Map | RenderMode::Landscape` enum. `v` key toggles between them. `draw()` dispatches to `draw_map_mode()` or existing `draw()`.

### Phase 2: Entity behavior glyphs

Files to modify:
- `src/game/render.rs` -- In the entity rendering loop within `draw_map_mode()`, compute glyph and color from `BehaviorState` + `Species` instead of reading `Sprite` component. The `Sprite` component remains the Landscape Mode appearance.
- Entity glyph resolution logic (pseudocode):
  ```
  fn map_mode_glyph(species, behavior_state) -> (char, Color) {
      match species {
          Villager => match behavior_state {
              Idle | Wander => ('@', CYAN),
              Gathering { Wood, .. } => ('$', BROWN),
              Gathering { Stone, .. } => ('$', GREY),
              Hauling { .. } => ('%', GOLD),
              Building { .. } => ('&', YELLOW),
              Farming { .. } => ('f', GREEN),
              Working { .. } => ('g', ORANGE),
              Sleeping { .. } => ('z', DIM_BLUE),
              FleeHome { .. } => ('!', RED),
              Exploring { .. } => ('?', LIGHT_GREEN),
              Captured => ('x', DARK_RED),
              Seek { .. } => ('@', YELLOW_GREEN),
              _ => ('@', CYAN),
          },
          Predator => match behavior_state {
              Hunting { .. } => ('W', BRIGHT_RED),
              _ => ('w', RED),
          },
          Prey => match behavior_state {
              FleeHome { .. } => ('!', ORANGE),
              Eating { .. } => ('r', GREEN_TINT),
              AtHome { .. } => HIDDEN,
              _ => ('r', BROWN),
          },
      }
  }
  ```

### Phase 3: Building center markers

Files to modify:
- `src/game/render.rs` -- After rendering terrain tiles, iterate completed buildings and draw their center marker glyph on top. Use `BuildingType` to select glyph.
- This only applies in Map Mode. Landscape Mode continues using the existing `Sprite` component on building entities.

### Phase 4: Polish and toggle

- Wire `v` key to cycle: Map -> Landscape -> Debug (existing).
- Ensure panel, minimap, overlays, build cursor, and query cursor all work identically in both modes.
- Remove weather particles from Map Mode rendering path.
- Test with screenshot harness: `cargo run --release -- --screenshot` should use Map Mode by default (or accept `--mode map`).

## Testing

- **Visual regression**: Use headless renderer to assert specific glyphs appear at expected positions for known game states.
- **Glyph uniqueness**: Unit test that no terrain glyph appears in the entity glyph table (except documented exceptions like `#` for wall/cliff).
- **Behavior coverage**: Test that every `BehaviorState` variant has a mapped glyph. Exhaustive match in the glyph function guarantees compile-time coverage.
- **Round-trip**: Render the same game state in both modes, verify entity counts match (same number of draw calls, different glyphs).

## Open Questions

1. **Should Map Mode show any time-varying information on terrain?** Candidates: farm growth stage (change `≡` color from brown to green as crops grow), resource depletion (berry bush `♦` dims as remaining drops). These are gameplay-relevant, not atmospheric.

2. **Aspect ratio handling.** Current renderer uses `CELL_ASPECT` (2 screen columns per world tile) for squarish pixels. Map Mode might look better at 1:1 (one character = one tile) since we're reading symbols, not painting landscapes. Worth testing both.

3. **Zoom levels.** At 1:1, a 120-column terminal shows 100+ tiles of width (minus panel). At 2:1, only 50. Map Mode arguably wants 1:1 for maximum information density. Landscape Mode wants 2:1 for visual quality. Should the mode toggle also change aspect ratio?

4. **Build site vs explorer glyph conflict.** Both use `?`. Options: (a) accept it (one moves, one doesn't), (b) build site uses `_`, (c) build site uses `?` but in a distinct color (muted yellow vs explorer's green).

5. **Particle effects.** Current smoke particles from workshops, construction dust, etc. Should these render in Map Mode? They add visual noise but also carry information ("that workshop is active"). Proposal: skip particles in Map Mode, rely on the `g` worker glyph instead.
