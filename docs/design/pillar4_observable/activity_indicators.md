# Activity Indicators

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 4 (Observable Simulation), supports Pillar 5 (Scale Over Fidelity)*

## Problem

A settlement with 50 villagers and a dozen buildings looks dead when the camera is zoomed out or when villagers are inside buildings. Processing buildings (Workshop, Smithy, Granary, Bakery) have a `worker_present` flag but the only visual sign of activity is the generic smoke particle that already exists -- grey dots drifting upward from any active `ProcessingBuilding`. There is no way to distinguish a working bakery from a working smithy at a glance. Farms being tended, buildings under construction, and active mining sites produce zero visual feedback.

The game design doc says: *"If you can't see it happening on screen, it doesn't count."* Right now, most production activity is invisible. A settlement at peak productivity looks the same as one where every villager is idle. The landscape rendering mode doc explicitly calls out "smoke from workshops, embers from smithies" as Rich-tier features, and entity_state_visibility lists activity indicators as a companion system.

The goal: a player should be able to look at the settlement from a distance and say "the bakery is running, the smithy is busy, someone is mining that mountain, and they're building something on the east side." The settlement breathes.

## Current Behavior

1. **Smoke particles exist but are generic.** `Game::step()` spawns grey smoke (`'.'` or `'\u{00b0}'`) from any `ProcessingBuilding` with `worker_present == true`. Character is randomly `.` or `°`, color is random grey `(100..180, 100..180, 100..180)`, lifetime 15-25 ticks, drift upward at -0.1 to -0.3 dy. All processing buildings produce identical smoke regardless of type.

2. **No particles for non-processing activities.** Building construction (`BehaviorState::Building`), mining/gathering (`BehaviorState::Gathering`), and farming (`BehaviorState::Farming`) produce no visual effects. A villager hammering at a build site is distinguishable only by their glyph (if entity_state_visibility is implemented) or by clicking to inspect.

3. **The `Particle` struct is simple and sufficient.** Fields: `x, y, ch, fg, life, dx, dy`. Particles move each tick, decrement life, and are removed at life=0. Rendered after entities. No background color, no size, no fade. This is adequate for the first pass.

4. **Weather particles (rain, snow) use a separate stateless system** in `draw_weather()` that scatters pseudo-random characters each frame. Activity indicators should use the stateful `Particle` system instead, since they are tied to specific world positions and need to persist across frames.

## Design

### Particle Types Per Activity

Each activity type gets a distinct particle signature defined by: character set, color palette, spawn position offset, velocity, lifetime, and spawn rate. The combination must be identifiable at a glance in both Map and Landscape modes.

#### Building Activity Particles

| Building | Activity | Chars | Color | Spawn Offset | Velocity (dx, dy) | Life | Rate |
|----------|----------|-------|-------|--------------|-------------------|------|------|
| Workshop | Processing (wood to planks) | `.` `°` `'` | `(140, 130, 110)` warm grey | (0, -1) above center | dx: -0.05..0.05, dy: -0.15..-0.08 | 18-28 | 1 in 3 ticks |
| Smithy | Processing (stone to masonry) | `*` `·` `'` | `(255, 140, 40)` to `(255, 80, 20)` orange-red | (0, -1) above center | dx: -0.08..0.08, dy: -0.25..-0.10 | 10-18 | 1 in 2 ticks |
| Granary | Processing (food to grain) | `.` `,` | `(180, 170, 120)` pale straw | (0, -1) above center | dx: -0.03..0.03, dy: -0.10..-0.05 | 12-20 | 1 in 4 ticks |
| Bakery | Processing (grain+wood to bread) | `~` `'` `.` | `(200, 200, 210)` white steam | (0, -1) above center | dx: -0.06..0.06, dy: -0.12..-0.06 | 20-35 | 1 in 2 ticks |

**Visual rationale:**
- **Workshop** keeps the current grey smoke -- sawdust and wood smoke. Slow, lazy drift. The most common building; it should be the baseline "something is working" signal.
- **Smithy** produces bright orange sparks/embers that rise fast and die quick. The hot color and rapid motion distinguish it instantly from grey workshop smoke. The `*` character reads as a spark.
- **Granary** produces minimal, pale particles -- dry grain processing is quiet work. Low rate, slow drift, short life. Almost invisible unless you look for it. Granaries are passive; they should not dominate the visual field.
- **Bakery** produces white steam that lingers and drifts wide. The `~` character suggests rising heat. Longer lifetime and wider horizontal drift create a visible plume that reads as "kitchen" from a distance. The white color contrasts with workshop grey.

#### Villager Activity Particles

These particles spawn at the villager's position (or the activity target) while the villager is in the corresponding `BehaviorState`.

| Activity | Chars | Color | Spawn Position | Velocity (dx, dy) | Life | Rate |
|----------|-------|-------|----------------|-------------------|------|------|
| Building | `#` `.` `+` | `(220, 200, 100)` dusty yellow | at build site target | dx: -0.15..0.15, dy: -0.10..0.10 | 6-12 | 1 in 4 ticks |
| Gathering(Stone) | `*` `'` `.` | `(200, 200, 220)` bright white-blue | at villager position | dx: -0.20..0.20, dy: -0.15..0.05 | 4-8 | 1 in 3 ticks |
| Gathering(Wood) | `.` `,` | `(139, 110, 60)` wood brown | at villager position | dx: -0.10..0.10, dy: -0.08..0.02 | 5-10 | 1 in 5 ticks |
| Gathering(Food) | `.` | `(80, 160, 60)` leaf green | at villager position | dx: -0.05..0.05, dy: -0.05..0.02 | 4-8 | 1 in 6 ticks |
| Farming | none | -- | -- | -- | -- | -- |

**Visual rationale:**
- **Building (construction)** throws dust and debris outward in all directions. The `#` character matches the build site glyph, creating visual coherence -- you see `#` fragments flying off the `#` site. Short life, wide spread. Reads as: "something is being hammered together."
- **Gathering(Stone) / mining** produces bright sparkle-flashes. The `*` reads as a spark from pickaxe on rock. White-blue color contrasts with grey mountain terrain. Short life and wide x-spread create a "chipping" effect. This is the most visually active gathering type because mining is dramatic.
- **Gathering(Wood) / chopping** produces subtle brown wood-chip particles that drift slightly downward (chips fall). Lower rate than mining because tree-felling is rhythmic, not constant impact.
- **Gathering(Food) / foraging** produces almost nothing -- a faint green dot occasionally. Foraging is quiet. The minimal particle avoids visual clutter from the most common early-game activity.
- **Farming** intentionally has NO particles. Farming is slow, quiet, repetitive work. The farm tiles themselves change visually (crop growth stages). Adding particles would create noise on what should be a calm, pastoral area. Farms are alive because of the crops, not because of flying debris.

### Interaction with Map Mode

In Map mode, characters carry semantic weight. Activity particles must not obscure the clean glyph grid.

**Rules for Map mode:**
1. **Building particles render.** Smoke, sparks, and steam from processing buildings are drawn above the building in the 1-3 tiles of open sky. They use the same characters and colors specified above. Since Map mode uses flat color (no lighting), particle colors are used directly without modification.
2. **Villager activity particles are suppressed.** In Map mode, the entity glyph itself already communicates activity (via entity_state_visibility -- `♠` for chopping, `⌐` for mining, `▒` for building). Adding particles on top would create clutter. The glyphs are the activity indicator in Map mode.
3. **Exception: construction dust.** Building construction particles still render in Map mode because the build site (`#`) is static -- the dust particles are the ONLY way to see that construction is actively happening vs. a queued but unworked site.

### Interaction with Landscape Mode

In Landscape mode, color is dominant and characters are texture. Activity particles are a natural fit.

**Rules for Landscape mode:**
1. **All particle types render.** Building particles and villager activity particles both appear. Landscape mode is the "watch" mode -- maximum visual richness.
2. **Particles receive lighting.** Particle fg colors are multiplied by the tile's light level at their current position. Smoke dims at night. Smithy sparks glow bright against dark terrain (their base color is already high-value, so even at 50% light they read as orange). Bakery steam catches moonlight as pale blue-white.
3. **Particles do NOT receive atmospheric perspective (fog).** Particles are ephemeral foreground effects. Fogging them would make nearby smoke look distant, which breaks the depth cue. Particles always render at full saturation regardless of distance from camera center.
4. **Night amplification for emissive particles.** Smithy sparks and embers are "emissive" -- they produce light, not reflect it. At night, their color is NOT dimmed by the light level. Instead, they render at full brightness, creating the effect of glowing sparks against dark terrain. Implementation: tag emissive particles with a flag, or simply exempt particle types with base color value > 200 from light multiplication.

### Particle Lifecycle

```
Spawn:
  - Each tick, iterate active ProcessingBuildings with worker_present == true
  - For each, roll against spawn rate (e.g., 1 in 3 chance per tick)
  - On success, create Particle with randomized values from the type's ranges
  - For villager activities, iterate entities with matching BehaviorState
  
Update (already exists):
  - p.x += p.dx
  - p.y += p.dy
  - p.life -= 1
  
Render (already exists):
  - Draw at (p.x, p.y) with p.ch, p.fg
  
Remove (already exists):
  - Retain only particles where p.life > 0
```

### Color Fade

Currently particles maintain constant color throughout their lifetime. For richer visuals, particle color should fade toward transparency as life approaches 0.

```
age_fraction = 1.0 - (p.life as f64 / p.max_life as f64)

// Fade: reduce color intensity in final 40% of life
if age_fraction > 0.6 {
    let fade = 1.0 - ((age_fraction - 0.6) / 0.4);  // 1.0 -> 0.0
    fg.0 = (fg.0 as f64 * fade) as u8;
    fg.1 = (fg.1 as f64 * fade) as u8;
    fg.2 = (fg.2 as f64 * fade) as u8;
}
```

This requires adding a `max_life` field to `Particle` (set to the initial `life` value at spawn). Alternatively, encode the initial life in the unused bits or just store it. The struct is already not `Copy` so adding a field is trivial.

### Spawn Rate Scaling

At 500+ villagers with many active buildings, particle count could explode. Cap total particles and reduce spawn rates at scale.

```
const MAX_PARTICLES: usize = 200;

// Before spawning new particles:
if self.particles.len() >= MAX_PARTICLES {
    // Skip spawning this tick, or only spawn for highest-priority types (smithy > bakery > workshop > villager)
    return;
}
```

200 particles at ~20 bytes each is 4KB -- negligible memory. The render cost is 200 draw calls per frame, also negligible compared to the ~5000+ tile draws per frame.

### Wind Interaction

Particle horizontal drift (`dx`) should incorporate wind direction when wind simulation exists. For now, use a small random drift. When wind is added:

```
p.dx += wind_x * 0.02;  // gentle push
p.dy += wind_y * 0.02;
```

This makes smoke plumes lean in the wind direction, which is a powerful atmospheric cue. Workshop smoke bending east tells the player "wind is blowing east" without any UI element.

## Extended Particle Struct

```rust
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub ch: char,
    pub fg: Color,
    pub life: u32,
    pub max_life: u32,      // NEW: initial life, for fade calculation
    pub dx: f64,
    pub dy: f64,
    pub emissive: bool,     // NEW: if true, not dimmed by night lighting
}
```

Two new fields. `max_life` enables color fade. `emissive` enables night glow for smithy sparks. Both default to `max_life = life` and `emissive = false` for backward compatibility with any existing particle spawning code.

## Implementation Plan

### Phase 1: Differentiate Processing Building Particles (Core)

Modify the existing smoke-spawning block in `Game::step()` (around line 1375 of `game/mod.rs`) to check the building's `Recipe` and spawn type-appropriate particles instead of generic grey smoke.

Key changes:
- `src/game/mod.rs`: In the smoke spawning loop, query `BuildingType` alongside `ProcessingBuilding` and `Position`. Match on building type to select character set, color range, velocity, lifetime, and spawn rate from a lookup table or match block.
- `src/game/mod.rs`: Add `max_life` and `emissive` fields to `Particle`. Set `max_life = life` at spawn. Set `emissive = true` for smithy sparks.
- `src/game/render.rs`: Apply color fade in the particle render pass based on `life / max_life`. Skip light dimming for `emissive` particles in Landscape mode.

**Done when:** Looking at a settlement with Workshop, Smithy, and Bakery running simultaneously, each has a visually distinct particle signature. Smithy glows orange, Bakery steams white, Workshop smokes grey.

### Phase 2: Villager Activity Particles

Add particle spawning for `Building`, `Gathering(Stone)`, and `Gathering(Wood)` behavior states.

Key changes:
- `src/game/mod.rs`: After the processing-building particle spawn block, add a second loop over entities with `BehaviorState::Building`, `BehaviorState::Gathering`. Spawn particles per the tables above, respecting spawn rate and `MAX_PARTICLES` cap.
- `src/game/mod.rs` or `src/game/render.rs`: In Map mode, suppress villager activity particles (except construction dust). Check `self.render_mode` before spawning or before drawing.

**Done when:** A mining operation on a mountainside produces visible white-blue sparkles. A construction site throws dust. These effects appear in Landscape mode and (for construction only) in Map mode.

### Phase 3: Lighting and Night Glow

Integrate particles with the Landscape mode lighting system.

Key changes:
- `src/game/render.rs`: In the particle render pass, when in Landscape mode, look up the light level at `(p.x, p.y)` from the precomputed light map. Multiply `p.fg` by light level unless `p.emissive`. This makes smoke dim at night and sparks glow.

**Done when:** At night in Landscape mode, the smithy is visible as a cluster of orange sparks against the dark terrain. Workshop smoke is barely visible. Bakery steam catches moonlight faintly.

### Phase 4: Polish and Scale

- Tune spawn rates and lifetimes by playtesting at various population scales (30, 100, 300 villagers).
- Implement `MAX_PARTICLES` cap with priority ordering.
- Add wind drift when wind simulation ships.

## Testing

- **Unit test: particle type differentiation.** Mock a world with one Workshop, one Smithy, one Bakery, all with `worker_present = true`. Run 30 ticks. Assert that spawned particles have distinct color ranges per building type (workshop grey < 180 for all channels, smithy red channel > 200, bakery all channels > 190).
- **Unit test: max_life and fade.** Spawn a particle with `life = 10, max_life = 10`. After 7 ticks (`life = 3`), verify that the rendered color is dimmer than the original (fade kicks in at 40% remaining life = life 4).
- **Unit test: emissive flag.** Verify smithy particles spawn with `emissive = true`, workshop particles with `emissive = false`.
- **Unit test: particle cap.** Fill particles to `MAX_PARTICLES`. Verify no new particles are spawned on the next tick.
- **Integration test: settlement looks alive.** Run `--play --ticks 1000` on a seed with active buildings. Assert `game.particles.len() > 0` at the final tick (settlement is producing visible activity).
- **Existing tests pass.** The two existing particle tests (`particles_spawn_from_active_workshop`, `particles_despawn_after_lifetime`) must still pass. The workshop test may need updating if the spawned particle characters/colors change.

## Interaction with Existing Systems

- **Entity state visibility (entity_state_visibility.md):** Complementary. Entity state visibility changes the ENTITY glyph to show what a villager is doing. Activity indicators add PARTICLES to the environment around the activity site. Together they create a layered readability system: the glyph tells you what the agent is doing, the particles tell you that the location is active.
- **Landscape rendering mode (landscape_rendering_mode.md):** Activity particles are listed as Rich-tier features in that doc ("smoke and embers near workshops/smithies"). This doc provides the concrete specification for those particles. The Landscape doc's smoke/ember specs (section "Embers" and "Smoke" under Weather Particle System) describe generic building effects -- this doc supersedes those with per-building-type differentiation.
- **Weather particles:** Weather uses a stateless per-frame system. Activity indicators use the stateful `Particle` system. They do not conflict. Both render in the same pass (weather via `draw_weather()`, activity via the particle loop in `draw()`). Render order: terrain -> weather -> entities -> activity particles. Activity particles render last so they appear on top of weather.
- **Day/night cycle:** Night dims the landscape. Emissive particles (smithy sparks) resist dimming, creating visible points of activity in the dark. This is the key payoff: at night, you can see which buildings are active from across the map by their glow.

## Open Questions

1. **Should farms have a subtle particle?** Current design says no -- farms communicate through crop growth stages. But a faint green shimmer during active tending could help distinguish "farm with active farmer" from "farm waiting for a worker." Worth prototyping.
2. **Garrison activity indicator?** Garrisons are not processing buildings, but an active garrison (with soldiers) could emit a subtle torch-light effect at night -- warm orange glow without particles. This overlaps with the Landscape doc's "building point lights" feature.
3. **Particle occlusion by buildings.** Should particles spawned inside a 3x3 building (Workshop, Smithy) appear to rise "from" the building, or should they spawn above the roof line? Current design spawns at `(center_x, center_y - 1)` which is the top wall tile. If half-block rendering ships, particles might need to spawn at `center_y - 1.5` to clear the roof visually.
4. **Sound cues.** The game design doc mentions terminal audio as a dream-tier feature. If implemented, smithy sparks could trigger a faint anvil ping, bakery steam a hiss. Pure speculation for now.
