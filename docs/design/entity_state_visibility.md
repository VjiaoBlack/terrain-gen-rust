# Entity State Visibility

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 4 (Observable Simulation)*

## Problem

Every villager renders as `V` in light blue (`Color(100, 200, 255)`) regardless of what they are doing. The only visual differentiation comes from the Tasks overlay, which color-codes villagers by activity -- but in normal play, a gatherer, a builder, a farmer, and a fleeing villager all look identical. You cannot answer "what is that villager doing?" at a glance.

The game design doc's success criterion for Pillar 4 is explicit: *"A new player can narrate what's happening in <30s"* and *"'What's that villager doing?' is answerable."* With the current single-glyph rendering, this fails.

Wolves have a similar problem. A wolf wandering, a wolf stalking prey, and a wolf mid-attack all render as `W` in red. You cannot see the threat escalating.

## Design Goals

1. **Every BehaviorState has a distinct visual representation** -- character, color, or both.
2. **Species behavior reads at a glance** -- villager states, predator states, and prey states are all distinguishable without needing the Tasks overlay.
3. **Both Map mode and Landscape mode benefit** -- Map mode uses distinct glyphs; Landscape mode uses color contrast and direction.
4. **No UI panels required** -- the entity's appearance IS the information. Overlays are supplementary, not essential.
5. **Direction matters** -- a fleeing villager should visually face away from the threat. A hauling villager should face toward the stockpile. An explorer should face the frontier.

## Current State

### Spawn Characters (from `spawn.rs`)

| Entity | Char | Color | Notes |
|--------|------|-------|-------|
| Villager | `V` | `(100, 200, 255)` light blue | Always `V`, never changes |
| Wolf (Predator) | `W` | `(160, 50, 50)` dark red | Always `W` |
| Rabbit (Prey) | `r` | `(180, 140, 80)` tan | Always `r` |
| Berry Bush | `♦` | `(200, 40, 80)` red | Static |
| Stone Deposit | `●` | `(150, 140, 130)` grey | Static |
| Stockpile | `■` | `(180, 140, 60)` gold | Static |
| Build Site | `#` | `(200, 180, 100)` tan | Static |
| Den | `O` | `(140, 100, 60)` brown | Static |

### Current Rendering (from `render.rs`)

- **Normal mode**: All entities render with their spawn sprite. The only behavioral modification is that sleeping villagers get a 0.5 dimming multiplier on their color.
- **Tasks overlay**: Colors villagers by state but does NOT change the character. Every villager is still `V`, just tinted. This requires the player to know the color code and to have the overlay active.
- **Threat overlay**: Draws wolves as bright red `W` and dens with a red zone. No behavioral distinction.

### All BehaviorState Variants (from `components.rs`)

| State | Used By | What It Means |
|-------|---------|---------------|
| `Wander { timer }` | All | Moving randomly, no goal |
| `Seek { target, reason }` | All | Moving toward a specific target |
| `Idle { timer }` | All | Standing still, waiting |
| `Eating { timer }` | Prey | Consuming food at a source |
| `FleeHome { timer }` | Prey, Villager | Running from a predator |
| `AtHome { timer }` | Prey | Safe in den, resting |
| `Hunting { target }` | Predator | Chasing prey |
| `Captured` | Prey | Frozen, about to be eaten |
| `Gathering { timer, resource_type }` | Villager | Harvesting at a resource site |
| `Hauling { target, resource_type }` | Villager | Carrying resources to stockpile |
| `Sleeping { timer }` | Villager | Resting at night in a hut |
| `Building { target, timer }` | Villager | Constructing at a build site |
| `Exploring { target, timer }` | Villager | Scouting toward the frontier |
| `Farming { target, lease }` | Villager | Tending crops at a farm |
| `Working { target, lease }` | Villager | Operating a workshop/smithy |

## Proposed Visual Representation

### Design Principles

**Character encodes activity.** What the entity IS DOING determines its glyph. A villager gathering wood is not the same character as a villager building.

**Color encodes resource/urgency.** What the entity is carrying or the urgency of their state determines color. Hauling wood is brown. Hauling stone is grey. Fleeing is red.

**Direction encodes intent.** When movement has a clear target, directional glyphs (`>`, `<`, `^`, `v` or Unicode arrows) show where the entity is headed. Fleeing faces AWAY from the threat. Hauling faces TOWARD the stockpile.

### Villager States

| BehaviorState | Map Mode Char | Map Mode Color | Landscape Mode Color | Mnemonic |
|---------------|---------------|----------------|----------------------|----------|
| `Idle` | `○` | `(80, 80, 180)` dim blue | dim blue, muted | Empty circle = nothing to do |
| `Wander` | `○` | `(100, 100, 160)` slate | slate, slightly brighter | Same as idle, slightly lighter |
| `Seek(Food)` | `!` | `(220, 180, 50)` amber | amber | Exclamation = hungry, urgent |
| `Seek(Stockpile)` | directional `→←↑↓` | `(200, 180, 50)` gold | gold | Arrow toward stockpile |
| `Seek(BuildSite)` | directional `→←↑↓` | `(255, 220, 50)` yellow | yellow | Arrow toward build site |
| `Seek(Wood)` | directional `→←↑↓` | `(139, 90, 43)` brown | brown | Arrow toward trees |
| `Seek(Stone)` | directional `→←↑↓` | `(150, 150, 150)` grey | grey | Arrow toward stone |
| `Seek(Hut)` | directional `→←↑↓` | `(100, 100, 200)` blue | blue | Arrow toward home |
| `Seek(ExitBuilding)` | `V` | `(100, 200, 255)` default | default | Brief transition, use default |
| `Seek(Unknown)` | `?` | `(150, 150, 50)` dim yellow | dim yellow | Unknown purpose |
| `Gathering(Wood)` | `♠` | `(139, 90, 43)` brown | brown, pulsing | Spade = chopping trees |
| `Gathering(Stone)` | `⛏` or `⌐` | `(150, 150, 150)` grey | grey, pulsing | Pickaxe = mining |
| `Gathering(Food)` | `♣` | `(50, 200, 50)` green | green, pulsing | Club = foraging |
| `Hauling(Wood)` | directional `→←↑↓` | `(180, 120, 50)` warm brown | brown with bright trail | Arrow + brown = carrying wood |
| `Hauling(Stone)` | directional `→←↑↓` | `(180, 180, 180)` light grey | light grey with bright trail | Arrow + grey = carrying stone |
| `Hauling(Food)` | directional `→←↑↓` | `(100, 220, 80)` bright green | green with bright trail | Arrow + green = carrying food |
| `Hauling(Grain)` | directional `→←↑↓` | `(220, 200, 80)` wheat | wheat with bright trail | Arrow + gold = carrying grain |
| `Hauling(Planks)` | directional `→←↑↓` | `(200, 160, 80)` tan | tan with bright trail | Arrow + tan = carrying planks |
| `Hauling(Masonry)` | directional `→←↑↓` | `(200, 200, 210)` off-white | off-white with bright trail | Arrow + white = carrying masonry |
| `Building` | `▒` | `(255, 220, 50)` bright yellow | yellow, pulsing | Half-block = construction |
| `Farming` | `∞` or `~` | `(80, 200, 80)` farm green | farm green, steady | Tilde = tending/tilling |
| `Working` | `⚙` or `*` | `(200, 120, 50)` workshop orange | orange, pulsing | Gear/star = crafting |
| `Exploring` | `►` or `>` | `(50, 180, 255)` bright cyan | cyan, bright against terrain | Arrow forward = scouting |
| `Sleeping` | `z` | `(60, 60, 140)` very dim blue | very dim, nearly invisible | z = sleeping (universal) |
| `FleeHome` | `!` | `(255, 50, 50)` bright red | bright red, high contrast | Exclamation + red = danger |

### Predator (Wolf) States

| BehaviorState | Map Mode Char | Map Mode Color | Landscape Mode Color | Mnemonic |
|---------------|---------------|----------------|----------------------|----------|
| `Wander` | `w` | `(160, 50, 50)` dark red | dark red, low-key | Lowercase = passive, not hunting |
| `Idle` | `w` | `(120, 40, 40)` dim red | dim red | Even dimmer when idle |
| `Seek(*)` | directional `→←↑↓` | `(180, 60, 60)` medium red | medium red | Moving with purpose |
| `Hunting` | `W` | `(255, 50, 50)` bright red | bright red, high contrast | UPPERCASE = active threat |
| `Eating` | `X` | `(200, 30, 30)` blood red | blood red | X = kill in progress |
| `FleeHome` | directional `→←↑↓` | `(160, 80, 80)` faded red | faded red | Retreating |

### Prey (Rabbit) States

| BehaviorState | Map Mode Char | Map Mode Color | Landscape Mode Color | Mnemonic |
|---------------|---------------|----------------|----------------------|----------|
| `Wander` | `r` | `(180, 140, 80)` tan | tan, natural | Lowercase = calm |
| `Idle` | `r` | `(140, 110, 60)` dim tan | dim tan | Dimmer when idle |
| `Eating` | `r` | `(100, 180, 60)` green-tan | green tint | Eating = near greenery |
| `FleeHome` | `!` | `(255, 200, 50)` bright yellow | bright yellow, fast | Exclamation = panicking |
| `AtHome` | `.` | `(100, 80, 50)` dim brown | nearly invisible | Dot = hidden in den |
| `Captured` | `x` | `(200, 50, 50)` red | red flash then fade | Lowercase x = small death |

## Direction Indicator System

For states that involve movement toward a target (Seek, Hauling, Exploring, Hunting), the character should reflect the dominant direction of travel.

### Implementation

Compute direction from current velocity or from `(position -> target)` vector:

```
if |dx| > |dy|:
    if dx > 0: '→' (or '>')
    else:      '←' (or '<')
else:
    if dy > 0: '↓' (or 'v')
    else:      '↑' (or '^')
```

For `FleeHome`, compute direction FROM the threat (the velocity is already pointing away), so the arrow naturally faces away from danger.

### Fallback Characters

Not all terminals render Unicode arrows well. Provide a fallback set:

| Unicode | ASCII Fallback |
|---------|---------------|
| `→` | `>` |
| `←` | `<` |
| `↑` | `^` |
| `↓` | `v` |
| `♠` | `T` (tree/timber) |
| `⛏` / `⌐` | `M` (mine) |
| `♣` | `F` (forage) |
| `▒` | `B` (build) |
| `⚙` / `*` | `*` |
| `►` | `>` |
| `∞` / `~` | `~` |

The renderer should detect terminal Unicode support or use a config flag. Default to ASCII fallback for maximum compatibility.

## Landscape Mode Specifics

In Landscape mode, characters are texture -- they don't carry as much semantic weight. Entity visibility in Landscape mode relies on:

1. **Color saturation contrast.** Terrain is muted; entities are saturated. A bright red wolf POPS against brown-green terrain. A bright cyan explorer stands out against muted landscape.

2. **Background highlight.** Active entities (Hunting wolves, Fleeing villagers, Builders) get a subtle background color behind them to create a "glow" effect:
   - Hunting wolf: `bg: Color(80, 0, 0)` dark red glow
   - Fleeing villager: `bg: Color(80, 0, 0)` danger glow
   - Building villager: `bg: Color(60, 50, 0)` construction glow
   - Exploring villager: `bg: Color(0, 40, 60)` frontier glow

3. **Sleeping villagers nearly vanish.** In Landscape mode, sleeping villagers should be barely visible -- they are inside huts, effectively hidden. Render at 20% brightness or skip entirely.

4. **Hauling villagers show a "trail" effect.** When a particle system exists, haulers leave 1-2 trailing particles in their wake colored to match their cargo. This makes resource flow lines visible at a distance.

## Implementation Plan

### Phase 1: Character Differentiation (core)

Modify the entity rendering pass in `render.rs` (around line 702 where `renderer.draw(sx, sy, sprite.ch, fg, None)` is called) to compute the display character from BehaviorState instead of always using `sprite.ch`.

```rust
// Pseudocode for the render pass
let (display_ch, display_fg) = entity_visual(species, behavior_state, velocity, sprite);
renderer.draw(sx, sy, display_ch, display_fg, bg);
```

Create a new function `entity_visual()` that takes species, behavior state, velocity, and default sprite, and returns the character and color to render. This keeps the rendering logic centralized and testable.

Key changes:
- `src/game/render.rs`: Replace `sprite.ch` with computed character in the entity draw pass.
- `src/game/render.rs`: Remove the Tasks overlay special-casing (lines 664-701) -- the information it provided is now the DEFAULT rendering, not an overlay. The Tasks overlay becomes redundant or can show additional detail.

### Phase 2: Direction Indicators

Add direction computation from velocity or target position. For `Seek`, `Hauling`, `Exploring`, and `Hunting` states, replace the character with a directional arrow.

Key changes:
- Add a helper `fn direction_char(dx: f64, dy: f64) -> char` in render.rs
- In `entity_visual()`, for movement states, use direction_char on the velocity or (target - position) vector

### Phase 3: Landscape Mode Polish

Add background color ("glow") for high-urgency states in Landscape mode. Adjust brightness curves so sleeping entities fade and active entities pop.

### Phase 4: Particle Trails (future)

Hauling villagers emit trailing particles matching their cargo color. Requires the particle system (already exists in render.rs line 707+) to support entity-spawned particles.

## Testing

- **Visual regression**: Run `cargo run --release -- --play --ticks 500` and verify villager states are distinguishable in the output.
- **Unit test**: `entity_visual()` is a pure function -- test that each BehaviorState produces the expected (char, color) pair.
- **Glance test**: The Pillar 4 success criterion. Show a frame to someone unfamiliar with the game. Ask "what is that entity doing?" for 5 random entities. Target: 4/5 correct without explanation.

## Interaction with Existing Systems

- **Tasks overlay**: Becomes redundant for basic activity identification. Could be repurposed to show more granular info (resource amounts carried, time remaining on gather/build, seek target distance).
- **Query/inspect mode** (`k` key): Already shows BehaviorState in text. No change needed.
- **Minimap**: Currently draws wolves as red dots and villagers as blue dots. Could be extended to use activity colors for villagers on minimap too, but this is low priority -- minimap is for location, not activity.
- **Sprite component**: The `Sprite.ch` field becomes the "base" character, used only when no behavior-specific override applies (e.g., for non-creature entities like buildings, bushes, deposits).

## Open Questions

1. **Unicode or ASCII default?** Most modern terminals handle basic Unicode, but some (especially on Windows) struggle with symbols like `♠`, `⚙`. Should we default to ASCII and let users opt into Unicode?
2. **Color-blind accessibility.** The current design relies heavily on color differentiation (brown vs grey vs green for resource types). Should we ensure character differences are sufficient alone? (Answer: yes, the character-first approach in Map mode handles this.)
3. **Crowded areas.** When 10 villagers occupy adjacent tiles, a wall of directional arrows might be noisy. Should we aggregate (show a single "group" indicator) at high density? This connects to Pillar 5 (Scale Over Fidelity) and the LOD rendering plan.
4. **Animation cycling.** Should gathering/building/working states cycle through 2-3 characters to suggest motion (e.g., `♠` -> `|` -> `♠` for woodcutting)? This adds life but increases rendering complexity.
