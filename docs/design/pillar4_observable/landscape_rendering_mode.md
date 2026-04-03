# Landscape Rendering Mode (Painterly Terminal)

*Design doc for Pillar 4, Mode B: "Landscape"*
*Status: Design*

## Problem Statement

The current renderer already has good bones: Blinn-Phong lighting with terrain normals, shadow sweep, ambient tinting, day/night cycle with moon, seasonal color shifts. But it applies this beautiful lighting to semantic characters (`'` `^` `~`) that fight for attention with the color field. The viewer reads characters AND colors simultaneously, and neither channel wins. The result is visual noise rather than atmosphere.

Mode A ("Map") solves this by leaning into clean symbolic glyphs with flat color. Mode B ("Landscape") solves it by going the opposite direction: color carries all meaning, characters become invisible texture.

## Vision

The player switches to Landscape mode and sees a painting. Rolling green hills with long shadows at dawn. Golden autumn forests. Snow-dusted tundra under cold blue moonlight. Villagers glow as warm dots against muted terrain. Smoke rises from workshops. Rain drifts across the screen. The terminal window becomes a porthole into a living world.

This is not the gameplay mode. This is the "fall in love" mode.

## Core Principle: Color Dominance

Every design decision in Landscape mode serves one rule: **the color field is the primary information channel.** Characters exist only to add surface texture -- they should be invisible at a glance. If you squint and blur the screen, you should still be able to read the terrain.

This means:
- Foreground and background colors are close in hue/brightness (low contrast text)
- Characters are chosen for visual density/weight, not semantic meaning
- Entities break the muted palette with saturated, high-contrast color
- Lighting modulates the entire color field uniformly -- shadows are darker versions of the same hue, not different colors

---

## Character Texture System

### Design Philosophy

Characters in Landscape mode are surface noise. They suggest roughness, density, organic variation. They are NOT symbols. A viewer should not be able to identify terrain by character alone -- only by color.

### Texture Sets Per Terrain Type

Each terrain type has a pool of characters. Per-tile selection is deterministic from position hash (so it does not flicker between frames) but appears random.

| Terrain     | Texture Pool             | Visual Weight | Notes                                     |
|-------------|--------------------------|---------------|-------------------------------------------|
| Grass       | `.` `'` `,` `` ` ``     | Light         | Sparse, airy. Whitespace-like             |
| Sand        | `.` `·` `,` `:` `` ` `` | Light         | Fine grain, dots                           |
| Desert      | `.` `·` `'` ` `         | Very light    | Sparse, empty feel                         |
| Forest      | `"` `:` `;` `%`         | Dense         | Thick, tangled. High ink coverage          |
| Scrubland   | `;` `'` `,` `:`         | Medium        | Between grass and forest                   |
| Mountain    | `^` `:` `#` `%`         | Heavy         | Craggy, rough texture                      |
| Cliff       | `#` `%` `|` `:`         | Very heavy    | Solid, imposing                            |
| Snow        | `.` `` ` `` `'` `,`     | Very light    | Barely visible characters. Near-blank      |
| Tundra      | `-` `.` `'` `,`         | Light         | Flat, barren                               |
| Marsh       | `,` `~` `.` `;`         | Medium        | Wavy, damp                                 |
| Water       | `~` `~` `~` `~`         | Animated      | Cycles: `~` `~` `~` (see Water section)  |
| Road        | `=` `-` `=` `-`         | Medium        | Slightly structured, human-made feel       |
| BuildingFloor | `+` `.` `+` `.`       | Light         | Regular pattern suggests human construction|
| BuildingWall  | `#` `#` `#` `#`        | Solid         | Keeps current solid appearance             |

### Position Hash for Texture Selection

```
texture_index = (wx * 7 + wy * 13 + wx * wy) % pool.len()
```

Cheap, deterministic, no flicker. The constants (7, 13) create enough visual disorder that adjacent tiles rarely share the same character.

### Vegetation Overlay Textures

Vegetation density (from VegetationMap) overrides base terrain texture:

| Density     | Texture Pool         | Notes                              |
|-------------|----------------------|------------------------------------|
| 0.0 - 0.2  | (base terrain)       | No vegetation visible              |
| 0.2 - 0.5  | `"` `,` `'` `;`     | Light scrub, sparse grass          |
| 0.5 - 0.8  | `%` `:` `"` `;`     | Brush, young trees                 |
| 0.8 - 1.0  | `%` `#` `&` `@`     | Dense canopy, heavy forest         |

---

## Color Palette System

### Design Philosophy

Hand-picked palettes per biome, per season. No algorithmic color -- every RGB value is chosen by eye. Each terrain has a foreground/background pair where fg is CLOSE to bg (low character contrast) and bg carries the biome identity.

The palette is organized as: `(fg, bg)` pairs. Lighting multiplies both uniformly.

### Base Palettes (Spring/Summer Neutral)

These are the "noon, clear day" colors. Lighting and season tinting modify from here.

```
Grass:
  fg: (55, 135, 55)    bg: (40, 110, 38)     -- warm green, fg barely visible

Sand:
  fg: (195, 175, 110)  bg: (180, 158, 95)    -- warm tan

Desert:
  fg: (210, 190, 130)  bg: (195, 175, 115)   -- pale gold

Forest:
  fg: (25, 85, 30)     bg: (15, 65, 18)      -- deep emerald

Scrubland:
  fg: (120, 115, 55)   bg: (100, 95, 42)     -- olive drab

Mountain:
  fg: (130, 120, 110)  bg: (105, 95, 85)     -- warm grey

Cliff:
  fg: (100, 92, 82)    bg: (75, 68, 58)      -- dark brown-grey

Snow:
  fg: (230, 230, 242)  bg: (215, 215, 228)   -- blue-white

Tundra:
  fg: (155, 165, 172)  bg: (138, 148, 155)   -- cold grey-blue

Marsh:
  fg: (50, 95, 65)     bg: (32, 72, 48)      -- murky green

Water (ocean):
  fg: (55, 100, 200)   bg: (25, 50, 120)     -- deep blue

Water (river/shallow):
  fg: (70, 130, 210)   bg: (40, 80, 150)     -- brighter blue

Road:
  fg: (155, 128, 78)   bg: (135, 110, 65)    -- packed earth

BuildingFloor:
  fg: (145, 125, 95)   bg: (115, 95, 72)     -- worked stone/wood

BuildingWall:
  fg: (165, 145, 115)  bg: (130, 112, 88)    -- structural material
```

### Seasonal Palette Shifts

Season modifies the base palette through additive/multiplicative adjustments. These stack with lighting.

**Spring** -- Fresh, bright, cool greens with slight blue cast:
```
Grass:      bg shift (+0, +15, +5)     -- brighter green
Forest:     bg shift (+0, +12, +5)     -- fresh emerald
Scrubland:  bg shift (+5, +10, +0)     -- new growth
Sand:       no change
Snow:       bg shift (+0, +0, -15)     -- melting, less blue
Marsh:      bg shift (+5, +15, +10)    -- wetter, more alive
```

**Summer** -- Warm, golden, saturated:
```
Grass:      bg shift (+10, +5, -10)    -- warm green, less blue
Forest:     bg shift (+5, +8, -5)      -- lush, dark
Desert:     bg shift (+15, +5, -10)    -- baked
Sand:       bg shift (+10, +5, -5)     -- warmer
Tundra:     bg shift (+10, +5, -5)     -- brief warmth
```

**Autumn** -- Orange/red shift on vegetation, warm browns elsewhere:
```
Grass:      bg becomes (110, 90, 35)   -- golden brown
Forest:     bg becomes (100, 55, 18)   -- deep orange-red
Scrubland:  bg becomes (120, 85, 30)   -- russet
Marsh:      bg shift (+15, -10, -10)   -- dying vegetation
Sand:       no change
Mountain:   bg shift (+5, +0, -5)      -- slightly warmer
```

**Winter** -- Desaturated, blue-shifted, snow overlay:
```
Grass:      bg becomes (140, 145, 155) -- frost/snow dusted
Forest:     bg becomes (50, 60, 55)    -- bare, dark, cold
Scrubland:  bg becomes (110, 108, 100) -- dead scrub
Marsh:      bg becomes (55, 65, 70)    -- frozen, grey
Sand:       bg shift (+20, +20, +30)   -- frost
Tundra:     bg becomes (195, 198, 210) -- heavy snow
Snow:       bg becomes (225, 225, 240) -- fresh powder
Mountain:   bg shift (+15, +15, +25)   -- snow-capped
```

### Elevation Color Gradient

Within a single biome, elevation subtly shifts color. This creates the "painted landscape" feel where you can read topography from color alone.

```
elevation_factor = (height - biome_min_height) / (biome_max_height - biome_min_height)

Low elevation:   bg shift toward (+0, +5, +10)   -- slightly bluer/cooler (valleys hold moisture)
Mid elevation:   no shift (base palette)
High elevation:  bg shift toward (+10, +5, -5)    -- slightly warmer/brighter (sun-facing slopes)
```

The shift is subtle: max 15 RGB units across the full elevation range. It should be felt, not seen.

### Moisture Color Gradient

Tiles with higher moisture values from the MoistureMap get a subtle green/blue shift:

```
moisture_factor = moisture_map.get(x, y).clamp(0.0, 1.0)

Dry (0.0):  no shift
Wet (1.0):  bg shift toward (-10, +8, +12)  -- cooler, slightly greener
```

This creates natural-looking variation within uniform biomes. A grassland near a river reads slightly different from grassland on a hilltop -- not because of a hard biome boundary, but because of continuous color flow.

---

## Lighting Integration

### Existing System (Keep and Enhance)

The current `DayNightCycle` system is already excellent for Landscape mode:
- Blinn-Phong with terrain normals: slopes facing the sun glow, slopes facing away darken
- Shadow sweep: mountains cast long shadows at dawn/dusk
- Ambient tint: warm orange at sunrise/sunset, cool blue at night, neutral midday
- Quantized to steps of 4 (prevents terminal flicker from tiny color changes)

### Landscape-Specific Lighting Adjustments

**Lower ambient floor.** Current ambient is 0.35, which keeps shadows readable in Map mode. For Landscape mode, drop to 0.20. Deeper shadows create more dramatic terrain relief. The color field does the work of readability, not character legibility.

```
// Map mode
let light = 0.35 + 0.65 * directional;

// Landscape mode
let light = 0.20 + 0.80 * directional;
```

**Warmer shadow tone.** Pure darkening makes shadows look dead. In Landscape mode, shadows get a slight blue/purple shift (simulating sky light bouncing into occluded areas):

```
// In shadow: instead of just darkening, shift toward (0.7, 0.75, 1.0)
let shadow_tint = (0.7, 0.75, 1.0);
let r = (base_r * shadow_tint.0 * shadow_intensity);
let g = (base_g * shadow_tint.1 * shadow_intensity);
let b = (base_b * shadow_tint.2 * shadow_intensity);
```

**Atmospheric perspective (distance fog).** Tiles far from the camera fade toward a sky-colored haze. This creates depth without z-levels.

```
let camera_center_x = camera.x + viewport_w / 2;
let camera_center_y = camera.y + viewport_h / 2;
let dist = ((wx - camera_center_x)^2 + (wy - camera_center_y)^2).sqrt();
let max_dist = viewport_diagonal / 2;
let fog_factor = (dist / max_dist).clamp(0.0, 0.6);  // max 60% fog

// Blend toward sky color
let sky = ambient_tint_as_color();  // (140, 160, 200) midday, shifts with time
let r = lerp(tile_r, sky.r, fog_factor);
let g = lerp(tile_g, sky.g, fog_factor);
let b = lerp(tile_b, sky.b, fog_factor);
```

Fog factor maxes at 0.6 so distant terrain is muted but not invisible. The sky color tracks the ambient tint, so at sunset the distant haze is orange, at night it is deep blue.

### Light Sources (Night)

Buildings with active workers emit warm point light at night. This creates pools of civilization in the dark landscape.

```
Torch/building light:
  color: (255, 180, 80)    -- warm amber
  radius: 6 tiles
  falloff: 1.0 / (1.0 + dist * 0.5)   -- soft inverse
  flicker: sin(tick * 0.3 + building_id) * 0.1  -- gentle pulse per building
```

At night, the contrast between warm building pools and cool moonlit terrain is the visual payoff. Villagers walking between lit areas and dark wilderness read as brave little dots venturing into the unknown.

Implementation: after computing solar/lunar light_map, iterate buildings and add point light contributions. The light_map stays as a single float per tile; point lights just add to it (clamped to 1.0).

---

## Entity Rendering

### The Contrast Principle

Terrain is muted. Entities are saturated. This is the entire entity visibility strategy in Landscape mode.

Terrain bg/fg colors live in a constrained gamut: low saturation, mid-range brightness. Entity colors are pushed to high saturation and higher brightness. The human eye tracks saturated spots on a desaturated field instantly.

### Entity Color Palette

These colors are for entity foreground. Entity background is always `None` (transparent to terrain bg underneath), so the entity character floats on the landscape.

```
Villager (idle):          (240, 220, 180)   -- warm cream/white
Villager (gathering):     (180, 220, 120)   -- spring green
Villager (building):      (220, 180, 100)   -- warm amber
Villager (farming):       (140, 200, 100)   -- earthy green
Villager (fleeing):       (255, 100, 80)    -- alarm red
Villager (carrying):      (200, 200, 140)   -- laden, slightly dim
Villager (sleeping):      (120, 120, 160)   -- cool, dormant

Wolf:                     (220, 60, 60)     -- aggressive red
Prey (rabbit):            (200, 200, 180)   -- soft, neutral
```

### Entity Characters

In Landscape mode, entity characters are simpler than Map mode. No directional arrows. The color tells you what, not the glyph.

```
Villager:     o           -- small, round, human
Wolf:         w           -- lowercase, predatory
Rabbit:       r           -- small
Building:     (current glyphs -- these ARE semantic and that's fine)
```

Buildings keep their current rendering (wall characters, floor patterns). They are part of the landscape -- permanent structures that become terrain-like. Only mobile entities use the color-contrast system.

### Entity Trail Particles (Optional, Rich Tier)

Villagers leave fading "trail" marks behind them as they move:

```
Trail character: '.'
Trail color: entity color at 30% opacity (blended with terrain)
Trail lifetime: 8 ticks, fading linearly
```

This creates subtle movement traces on the landscape -- you can see where activity is happening even if you missed the villager passing. Combined with the traffic map data that already exists, frequently walked paths develop a visible "wear" pattern.

---

## Weather Particle System

### Rain

Spawns falling characters across the viewport. Characters drift downward with slight wind offset.

```
Rain characters: '|' ',' '.' ':'
Rain color: (140, 160, 200) at 40% blend with tile  -- subtle blue-grey
Density: 5-15% of viewport tiles per frame (scales with rain_rate from SimConfig)
Fall speed: 1 tile per frame, slight horizontal drift from wind
Lifetime: 1 frame (stateless -- just random placement each frame)
```

Rain is rendered AFTER terrain, BEFORE entities. Entities punch through rain (they are always on top).

### Snow

Similar to rain but slower, denser, and white.

```
Snow characters: '*' '.' ',' '`'
Snow color: (220, 225, 240) at 30% blend
Density: 3-10% of viewport tiles
Fall speed: 1 tile per 2-3 frames (slower than rain)
Drift: more horizontal wander than rain (snowflakes meander)
```

Snow particles accumulate visually during winter: the snow terrain palette shift handles this, but seeing flakes fall while the ground whitens ties cause to effect.

### Dust/Wind (Desert/Scrubland)

```
Dust characters: '.' ',' '`'
Dust color: (190, 170, 120) at 25% blend
Direction: horizontal (wind direction from simulation if available, else east)
Density: 2-5% of viewport tiles on desert/scrubland terrain only
```

### Embers (Near Active Workshops/Smithies)

```
Ember characters: '.' '*' ','
Ember color: (255, 160, 60) to (255, 80, 20) -- orange to red
Radius: 3-4 tiles from building
Rise: upward 1 tile per frame
Lifetime: 3-6 frames, color cools as it fades
```

### Smoke

```
Smoke characters: '.' ',' ':'
Smoke color: (160, 155, 150) at 20% blend -- barely there
Radius: 2-5 tiles above buildings with active production
Rise: upward 1 tile per 2 frames (slow)
Drift: slight wind offset
```

---

## Seasonal Atmosphere

Beyond palette shifts, each season applies a full-screen tint overlay and changes particle behavior.

### Spring
- **Tint:** slight green cast `(0.95, 1.02, 0.95)` multiplied into final color
- **Particles:** occasional rain. No snow. Light wind.
- **Special:** water bodies shimmer more (increased shimmer amplitude)

### Summer
- **Tint:** warm golden `(1.05, 1.0, 0.90)` -- everything slightly amber
- **Particles:** dust on desert tiles. No rain. Heat shimmer (see Advanced section).
- **Special:** shadows are shorter (sun higher), midday is very bright

### Autumn
- **Tint:** warm orange `(1.05, 0.95, 0.85)` -- the "golden hour all day" feel
- **Particles:** leaves falling on forest tiles (reuse snow mechanics, orange/red/brown colors, `'` `,` characters)
- **Special:** morning fog overlay (see Fog section)

### Winter
- **Tint:** cold blue `(0.85, 0.90, 1.05)` -- everything desaturated, blue-shifted
- **Particles:** snow. Breath puffs near villagers (tiny white `.` above entities every few ticks).
- **Special:** water tiles freeze (static character, white-blue palette). Nights are longer (already handled by DayNightCycle tick_rate or could adjust sun_elevation curve seasonally).

### Fog Overlay

Active during autumn mornings and spring dawns. Reduces contrast globally and adds a white-ish wash.

```
fog_intensity = base_fog * time_factor * season_factor

// base_fog: 0.0 - 0.4 (randomized per day)
// time_factor: peaks at 6-8 AM, gone by 10 AM
// season_factor: 1.0 in autumn, 0.5 in spring, 0.0 otherwise

// Application: blend every tile toward fog color
fog_color = (180, 185, 195)  -- cool grey
final = lerp(tile_color, fog_color, fog_intensity)
```

Fog + atmospheric perspective stack. A foggy autumn morning should make distant terrain nearly disappear while nearby trees glow orange in the first light. This is the money shot.

---

## Advanced Techniques

### Half-Block Characters (Double Vertical Resolution)

Terminal cells are roughly 2:1 (tall:wide). The upper-half `\u{2580}` and lower-half `\u{2584}` block characters let us pack TWO vertical pixels per cell by setting one pixel's color as foreground and the other as background.

```
// Upper half block: foreground = top pixel, background = bottom pixel
renderer.draw(x, y, '\u{2584}', bottom_color, Some(top_color));
```

This effectively doubles our vertical resolution. Instead of one color per cell, we get a 2x1 pixel per cell. For Landscape mode this is transformative: terrain gradients become twice as smooth, entity rendering becomes more precise.

**Implementation approach:**
- Compute the color for world tile at `(wx, wy)` (top half) and `(wx, wy+1)` (bottom half)
- Use `\u{2584}` (lower half block) with fg = bottom_tile_color, bg = top_tile_color
- Each screen row now represents TWO world rows
- Effectively doubles the visible map area vertically, or keeps same area at 2x resolution

**Trade-offs:**
- Characters (entities, text) require special handling -- they occupy both halves
- Entity rendering needs to decide: top half, bottom half, or full cell
- Not all terminals render half-blocks perfectly (test with major terminals)
- Could be a sub-mode toggle: `Landscape` vs `Landscape HD`

### Braille Dot Characters (Near-Pixel Rendering)

Unicode braille characters `\u{2800}` - `\u{28FF}` encode a 2x4 dot matrix per character cell. Each cell becomes 8 binary pixels. Combined with foreground color, this gives us 2x4 monochrome pixels per cell with colored foreground.

```
Braille cell layout:
  1  4
  2  5
  3  6
  7  8

Character = 0x2800 + (dot1 * 1 + dot2 * 2 + dot3 * 4 + dot4 * 8 + ...)
```

**Use cases in Landscape mode:**
- Particle rendering at sub-cell precision (raindrops between cell boundaries)
- Smooth coastline rendering (water/land boundary isn't jagged)
- Entity position at sub-tile precision (villager appears to move smoothly between cells)
- Minimap rendering (entire world in a small panel, 8 tiles per character)

**Limitation:** braille only has one foreground color per cell. For terrain rendering, this means a cell can only show one biome color. Best used for overlay effects (particles, paths) rather than base terrain.

### Parallax Depth Layering

Create an illusion of depth by rendering terrain at different "speeds" based on elevation when the camera pans.

```
// During camera scroll, offset high-elevation tiles less than low-elevation:
let parallax_offset = (tile_elevation - sea_level) * parallax_strength;
let render_x = screen_x + (camera_dx * parallax_offset) as i32;
let render_y = screen_y + (camera_dy * parallax_offset) as i32;
```

This is subtle and potentially disorienting. Worth prototyping but may not ship. A simpler version: just use atmospheric perspective (already described above) which achieves depth feeling without geometric distortion.

### Heat Shimmer (Summer)

On hot tiles (desert, scrubland) in summer midday, vertically adjacent tile pairs occasionally swap for one frame:

```
if season == Summer && hour > 10.0 && hour < 14.0 && terrain.is_hot() {
    if hash(wx, wy, tick) % 20 == 0 {
        // Swap this tile's color with the tile above
        // Creates a "wavy" heat distortion effect
    }
}
```

Very subtle. 5% of hot tiles per frame. Creates a shimmering mirage effect without any new rendering infrastructure.

### Water Reflection

Tiles immediately adjacent to water bodies (shore tiles) get a faint reflection of whatever is above them, color-shifted toward blue:

```
if adjacent_to_water(wx, wy) {
    let reflected_color = get_tile_color(wx, wy - 1);  // tile "above" in world
    let reflection = blend(reflected_color, water_color, 0.7);  // 70% water, 30% reflected
    // Apply as slight tint to this shore tile's water edge
}
```

This is a very cheap effect that adds surprising realism at shorelines.

---

## Rendering Pipeline (Landscape Mode)

The full per-frame rendering order:

```
1. Clear frame buffer
2. Draw panel (unchanged from current)
3. For each visible tile (sx, sy):
   a. Resolve terrain type at (wx, wy)
   b. Look up base palette for terrain + season
   c. Apply elevation gradient shift
   d. Apply moisture gradient shift
   e. Apply seasonal tint multiplier
   f. Apply DayNightCycle lighting (Blinn-Phong + shadows)
   g. Apply atmospheric perspective (distance fog)
   h. Apply fog overlay (if active)
   i. Select texture character from terrain pool (position hash)
   j. Apply vegetation overlay (if density > 0.2, override character + shift fg)
   k. Draw cell: (character, fg, bg)
4. Draw water tiles (animated characters, shimmer, seasonal freeze)
5. Draw weather particles (rain/snow/dust/embers/smoke)
6. Draw entity trails (if enabled)
7. Draw entities (saturated color, simple glyphs)
8. Draw building point lights (night only, additive)
9. Draw overlays (territory, traffic, etc. -- same as current but color-shifted for Landscape palette)
10. Draw status bar
11. Flush (crossterm double-buffer diff)
```

Steps 3a-3k happen per-tile and should be fast. The expensive parts (lighting computation, shadow sweep) already run before rendering in `compute_lighting()`. Most per-tile work is table lookups and simple arithmetic.

---

## Implementation Plan

### Tier 1: Core (Make It Work)

1. **Landscape palette tables.** New `fn landscape_fg()` and `fn landscape_bg()` on Terrain returning the muted palette from this doc. Parallel to existing `fg()`/`bg()`.
2. **Texture character selection.** New `fn landscape_ch()` on Terrain that takes `(wx, wy)` and returns a character from the texture pool via position hash.
3. **Render mode flag.** Add `RenderMode::Landscape` variant. The `draw()` method branches: Map mode uses current `ch()`/`fg()`/`bg()`, Landscape mode uses new methods.
4. **Seasonal palette application.** Move season_tint logic to use the concrete palette shifts from this doc instead of additive RGB math. Season-aware palette lookup.
5. **Entity color-by-state.** In Landscape mode, entity color is determined by `BehaviorState`, not just `Species`. Saturated palette as specified above.
6. **Lower ambient in Landscape mode.** Adjust the ambient floor in `apply_lighting` when mode is Landscape.

**Done when:** Pressing `v` toggles between Map and Landscape modes. Landscape mode shows muted terrain with texture characters, entities pop by color, lighting and shadows work. Looks noticeably different from Map mode.

### Tier 2: Rich (Make It Beautiful)

7. **Atmospheric perspective.** Distance fog blending toward sky color. Integrates into the per-tile render loop.
8. **Weather particles.** Rain, snow, dust. Stateless per-frame random placement. Rendered after terrain, before entities.
9. **Building point lights at night.** Warm glow pools around active buildings.
10. **Fog overlay.** Autumn mornings, spring dawns. Global contrast reduction + white wash.
11. **Elevation and moisture color gradients.** Subtle continuous color variation within biomes.
12. **Shadow color shift.** Blue/purple shadow tint instead of pure darkening.
13. **Smoke and embers.** Near workshops and smithies.

**Done when:** Watching a full day/night cycle through all four seasons in Landscape mode is genuinely atmospheric. Dawn, rain, autumn fog, winter snow all feel distinct.

### Tier 3: Dream (Push the Terminal)

14. **Half-block rendering mode.** `Landscape HD` sub-mode using `\u{2584}` for double vertical resolution.
15. **Braille particle rendering.** Sub-cell precision for weather particles.
16. **Heat shimmer.** Summer desert distortion effect.
17. **Water reflection at shorelines.** Faint color echo.
18. **Entity trail particles.** Movement traces that fade over time.
19. **Parallax depth on camera pan.** Subtle elevation-based scroll offset.

**Done when:** Screenshots of the terminal look like art. People share them.

---

## Performance Considerations

- Palette lookups are table-driven, not computed. Per-tile cost: ~5 table lookups + arithmetic. Negligible.
- Weather particles are stateless: random placement per frame, no particle state to track. Cost: O(viewport_size * density). At 5% density on a 200x50 viewport, that is 500 extra draw calls per frame.
- Atmospheric perspective is one `lerp` per tile. The distance computation can be precomputed per-frame (only camera position changes it).
- Half-block mode halves the number of terminal cells to update (2 world rows per screen row) but doubles the world rows visible. Net: similar cell count, fewer terminal writes (good for flush performance).
- The crossterm double-buffer already skips unchanged cells. Landscape mode may have MORE unchanged cells per frame (muted colors change less than bright ones from small lighting shifts), improving flush performance.

## Testing Strategy

- **Visual regression via headless renderer.** Render a known seed in Landscape mode to the headless buffer. Assert specific tiles have expected palette colors. This catches palette table errors.
- **Seasonal snapshot tests.** Render the same viewport in all 4 seasons, verify color shifts match the spec.
- **Entity contrast test.** For each entity state, verify that entity fg color has a minimum contrast ratio against every terrain bg color. Target: WCAG AA (4.5:1) or better for all entity-on-terrain combinations.
- **Character pool coverage test.** Verify every terrain type returns valid characters from its texture pool for a range of position hashes.
- **Performance benchmark.** Time a full Landscape render of a 200x50 viewport. Target: under 2ms for the render pass (excluding flush).

---

## Open Questions

- **Vegetation character density vs terrain character density.** Should dense forest (`%` `#` `&`) override or blend with mountain texture when forest grows on mountain slopes? Current leaning: vegetation wins if density > 0.5.
- **How does exploration fog interact with Landscape mode?** Current `'░'` on dark background is Map-flavored. Landscape fog-of-war might use solid dark tiles with slight noise, or a gradient edge.
- **Entity size at scale.** At 500+ villagers, do we switch to density coloring instead of individual entity rendering? In Landscape mode, a colored "heat" patch might look better than 50 overlapping `o` characters.
- **Terminal compatibility.** Half-block and braille characters require Unicode support. Should Landscape mode detect terminal capabilities and gracefully degrade?
- **Colorblind accessibility.** The color-dominant design is inherently harder for colorblind players. Do we need a Landscape mode variant with adjusted palettes? Or is Map mode the accessible mode and that is sufficient?
