# Terminal Visual Design Research

**Date:** 2026-04-04
**Goal:** State-of-the-art techniques for making terrain-gen-rust look *beautiful* in a terminal.
**Context:** We already have Blinn-Phong lighting, shadow sweep, day/night cycle, seasonal tinting, and 24-bit true color via crossterm. This document catalogs techniques to push further.

---

## 1. Reference Games and What Makes Them Work

### Brogue — The Gold Standard for ASCII Atmosphere

Brogue's beauty comes from **dynamic per-cell lighting with color blending**. Key techniques:

- **Color struct with randomness:** Each color has base RGB + random variance per channel + a `colorDances` flag for animation. This means grass isn't one green — it's a *range* of greens that shimmer.
- **Light source system:** 50+ named light types (fire, lava, wisp, fungus, spectral...). Each has a color, radial fade percentage, and radius range. Light *adds* to existing cell color, creating natural blending.
- **Directional per-side lighting:** Tracks light reaching each *side* of each cell (not just binary lit/unlit). Walls appear lit on the side facing the light and dark on the opposite side. Corner brightness also tracked for room corners.
- **Light channels:** Bitfield system — each source declares which channels it illuminates, each entity declares which channels it receives. Prevents self-illumination artifacts (e.g., a lighthouse doesn't light itself).
- **Constants:** `LIGHT_SMOOTHING_THRESHOLD: 150`, `VISIBILITY_THRESHOLD: 50` — tuned values that give the signature soft-glow look.

**What we could adopt:** Our Blinn-Phong is already more physically-based than Brogue's system. What we lack is **per-cell color variance** (random noise on terrain colors) and **multiple light sources** (campfires, torches in buildings, bioluminescent swamps). Adding even 2-3 point lights would dramatically increase atmosphere.

Sources:
- [BrogueCE Source — Rogue.h](https://github.com/tmewett/BrogueCE/blob/master/src/brogue/Rogue.h)
- [Angband Forums — true color ASCII bRogue-style](https://angband.live/forums/forum/angband/development/6213-true-color-ascii-graphics-brogue-style)
- [Gridbugs — Roguelike Lighting Demo](https://www.gridbugs.org/roguelike-lighting-demo/)

### Cogmind — Mastering Visual Hierarchy in ASCII

Cogmind proves ASCII can look *professional*. Key lessons from Grid Sage Games:

- **Grayscale first, color second:** All art starts in grayscale to nail form and weight. Color is a final pass, using only 1-2 colors per element. This prevents the "rainbow vomit" problem.
- **Glyph weight/density:** Characters have visual "weight" (pixel fill). Dense clusters create emphasis; sparse areas recede. Conscious management of density across the screen.
- **Negative space as design tool:** When nothing is happening, lots of black. This makes movement and important details pop through contrast.
- **Two-cell-wide glyphs:** Uses two adjacent cells per tile for square aspect ratio. We already do this with `CELL_ASPECT`.
- **Weapon color theming:** Green/yellow/blue for energy weapons, orange/red/purple for powerful ones, brown/white for ballistic. Consistent color = instant recognition.
- **REXPaint workflow:** All art created in REXPaint (free ASCII editor) — design mockups *before* implementing.

**What we could adopt:** Our terrain colors are good but we could strengthen **visual hierarchy** — ensure entities always pop against terrain, use brightness/saturation contrast not just hue. Consider a REXPaint mockup pass for UI layout.

Sources:
- [Cogmind ASCII Art, the Making of](https://www.gridsagegames.com/blog/2014/03/cogmind-ascii-art-making/)
- [Cogmind ASCII Art Gallery](https://www.gridsagegames.com/blog/2014/12/cogmind-ascii-art-gallery/)
- [Roguelike Development with REXPaint](https://www.gamedeveloper.com/design/roguelike-development-with-rexpaint)

### Caves of Qud — Restricted Palette, Maximum Character

- **18 fixed colors** with clear naming (Qud Viridian, True Black, etc). Each tile gets exactly primary + detail + background (3 colors max).
- **CRT effects:** Scan lines + vignetting applied as post-processing, giving the feeling of viewing through an old terminal. This is cheap to implement.
- **Palette available:** [Qud Viridian on Lospec](https://lospec.com/palette-list/qud-viridian)
- **Font choice:** Source Code Pro for the tileless/ASCII mode.

**What we could adopt:** The CRT effect idea is interesting for a "retro" toggle. The strict 18-color constraint is elegant but too restrictive for our terrain gradients. However, constraining our *UI panel* to a limited palette while terrain uses full 24-bit could give the best of both worlds.

Sources:
- [Caves of Qud Visual Style Wiki](https://wiki.cavesofqud.com/wiki/Visual_Style)
- [Caves of Qud Color Codes & Object Rendering](https://wiki.cavesofqud.com/wiki/Modding:Colors_&_Object_Rendering)
- [Dromad-vim — Qud palette for vim](https://github.com/ngscheurich/dromad-vim)

### Stone Story RPG — Frame-by-Frame ASCII Animation

- **10,000+ hand-drawn frames** of ASCII animation. Each frame is a .txt file.
- **Material language:** Symbol combinations consistently represent materials (wood, metal, stone). Players learn to "read" the visual language.
- **256 characters** (extended ASCII, not just 128).
- **Audio-visual synergy:** Sound fills gaps where visuals can't convey detail.

**What we could adopt:** We don't need frame-by-frame animation, but the **material language** concept is powerful. Define consistent character vocabularies: `~≈∼` = water, `'"` = vegetation, `^▲△` = mountains, etc. We partially do this already.

Sources:
- [Stone Story RPG ASCII Tutorial](https://stonestoryrpg.com/ascii_tutorial.html)
- [Road to IGF: Stone Story RPG](https://www.gamedeveloper.com/business/road-to-the-igf-martian-rex-standardcombo-s-i-stone-story-rpg-i-)
- [Stone Story RPG Interview](https://www.indiegraze.com/2018/09/22/interview-stone-story-rpgs-gabriel-santos/)

### Terminal Rain — Weather Effects in ASCII

A cyberpunk roguelike demonstrating rain, lightning, and dynamic weather as ASCII visual effects. Demonstrates that weather is achievable and impactful in terminal games.

Source: [Terminal Rain on TIGSource](https://forums.tigsource.com/index.php?topic=47529)

---

## 2. Color Palettes for Terminal Games

### Gruvbox — Retro Groove

- Philosophy: warm, earthy, "retro groove" feel
- Three contrast levels: soft, medium (default), hard
- Dark mode BG: `#282828` (hard), `#32302f` (medium), `#3c3836` (soft)
- Key accents: Red `#cc241d`, Green `#98971a`, Yellow `#d79921`, Blue `#458588`, Purple `#b16286`, Aqua `#689d6a`, Orange `#d65d0e`
- **Terrain fit:** The earthy warm tones are excellent for terrain. Green-to-brown gradient matches natural elevation progression.

### Catppuccin — Soothing Pastels

- 4 flavors: Latte (light), Frappe, Macchiato, Mocha (darkest)
- 26 colors per flavor, all hand-crafted
- Design principles: "Colorful > colorless", "Balance: not too dull, not too bright", "Harmony > dissonance"
- Mocha base: `#1e1e2e`, Surface: `#313244`, Text: `#cdd6f4`
- Key accents: Rosewater `#f5e0dc`, Flamingo `#f2cdcd`, Pink `#f5c2e7`, Mauve `#cba6f7`, Red `#f38ba8`, Peach `#fab387`, Yellow `#f9e2af`, Green `#a6e3a1`, Teal `#94e2d5`, Sky `#89dceb`, Sapphire `#74c7ec`, Blue `#89b4fa`, Lavender `#b4befe`
- **Terrain fit:** The pastels might be too soft for gritty terrain, but the *approach* of careful harmony is worth studying. Good for UI elements.

### Practical Recommendation for terrain-gen-rust

Use a **split palette strategy:**
1. **Terrain:** Full 24-bit RGB with Blinn-Phong lighting. Earthy base colors (we already have this).
2. **UI panel:** Constrained palette inspired by Gruvbox warmth — dark warm background `(25,25,40)` already close.
3. **Entities:** Saturated, bright colors that pop against muted terrain (Cogmind approach). We already do this well.
4. **Highlights/alerts:** Single accent color family (warm yellow/orange for important, red for danger).

Sources:
- [Let's Create a Terminal Color Scheme — Ham Vocke](https://hamvocke.com/blog/lets-create-a-terminal-color-scheme/)
- [Catppuccin Palette](https://catppuccin.com/palette/)
- [Gruvbox Color Palette — DeepWiki](https://deepwiki.com/morhetz/gruvbox/3.1-color-palette)

---

## 3. Unicode Character Techniques for Higher Resolution

### Half-Block Characters (The Big Win)

**Technique:** Use `▀` (upper half block, U+2580) or `▄` (lower half block, U+2584) with foreground = one pixel color, background = other pixel color. This **doubles vertical resolution** — each terminal cell represents two stacked pixels.

```
// Two pixels per cell:
// Upper pixel = background color
// Lower pixel = foreground color of ▄
// Result: 2x vertical resolution
```

**Impact:** An 80x24 terminal becomes effectively 80x48 pixels. For a minimap or overview mode, this would look dramatically better than single-character terrain.

**Implementation:** Trivial. Render pairs of rows, choosing fg/bg colors for each half-block. The crossterm renderer already supports per-cell fg+bg.

Sources:
- [Terminal Pixel Art — Lucamug on Medium](https://lucamug.medium.com/terminal-pixel-art-ad386d186dad)
- [Half-Height Console Graphics with Haskell](https://a.skh.am/2020/11/26/half-height-console-graphics.html)
- [Pixterm — true color pixel art in terminal](https://github.com/eliukblau/pixterm)

### Braille Characters (U+2800–U+28FF) — Maximum Resolution

Each braille character is a 2x4 binary pixel grid = **8 sub-pixels per cell**. An 80x24 terminal becomes 160x96 effective pixels.

**Encoding:**
```
Bit positions in a braille cell:
  (0,0)=bit0  (1,0)=bit3
  (0,1)=bit1  (1,1)=bit4
  (0,2)=bit2  (1,2)=bit5
  (0,3)=bit6  (1,3)=bit7

char = 0x2800 + bitfield
```

**Limitation:** Only binary (on/off) per cell since braille dots share a single fg/bg color pair. Good for contour lines, mini-maps, graph overlays. Not for full terrain rendering.

**Practical use for us:** Elevation contour overlay, minimap, flow direction visualization.

Sources:
- [Drawille — pixel graphics in terminal with braille](https://github.com/asciimoo/drawille)
- [termdot — 2x4 pixel grid per cell](https://github.com/ahmadawais/termdot)
- [Unicode Graphics — Dernocua](https://dernocua.github.io/notes/unicode-graphics.html)

### Quadrant Characters (U+2596–U+259F) — 2x2 Pixels

`▖▗▘▙▚▛▜▝▞▟` — four sub-pixels per cell. Less resolution than braille but can use color. Combined with fg+bg, you get 2x2 pixels with 2 colors per cell.

### Block Shade Characters — Density Gradients

`░▒▓█` (U+2591–U+2593, U+2588) provide four density levels. We already use `░` for building floors. These are excellent for:
- Fog of war gradients (explored but not visible = `░`, unexplored = space)
- Vegetation density (sparse `░`, medium `▒`, dense `▓`)
- Depth/distance fog

### Box Drawing (U+2500–U+257F) — Structural Elements

Light `─│┌┐└┘├┤┬┴┼`, heavy `━┃┏┓┗┛┣┫┳┻╋`, double `═║╔╗╚╝╠╣╬`, rounded `╭╮╰╯`.

**Use for:** UI panel borders, road rendering (roads as connected line segments), river rendering, building outlines.

---

## 4. Lighting Techniques — Making Terrain Look 3D

### What We Already Have (Strong Foundation)

Our `day_night.rs` implements:
- **Blinn-Phong shading** with terrain normals from central finite differences
- **Shadow sweep** — O(cells) single-pass shadow propagation against sun direction
- **Sun + moon** with proper elevation/azimuth, seasonal day length
- **Ambient tint** — warm sunrise/sunset, blue twilight, silver moonlight
- **Specular attenuation** — reduced when sun is high to avoid uniform wash
- **Normal scale = 20** — tuned for visible hill contrast without over-darkening

### Techniques to Add

#### A. Per-Cell Color Noise (Brogue-style)
Add small random variance to terrain base colors. Each tile gets a deterministic random offset (seeded by position) to its RGB values. This breaks up the flat look of uniform terrain.

```rust
// Pseudocode
let noise = hash(wx, wy) % 16 - 8; // -8 to +7
let fg = Color(
    (base.r as i16 + noise).clamp(0, 255) as u8,
    (base.g as i16 + noise).clamp(0, 255) as u8,
    (base.b as i16 + noise).clamp(0, 255) as u8,
);
```

Cost: nearly zero. Impact: significant texture improvement.

#### B. Ambient Occlusion Approximation
Darken tiles that are "enclosed" — surrounded by higher terrain or dense forest. Simple kernel: average the height difference to the 8 neighbors. If the tile is in a valley (all neighbors higher), darken it.

```rust
let ao = neighbors.iter()
    .map(|n| (n.height - center.height).max(0.0))
    .sum::<f64>() / 8.0;
let ao_factor = 1.0 - (ao * 2.0).min(0.3); // darken by up to 30%
```

#### C. Heightmap-Based Character Selection
Instead of fixed chars per terrain type, modulate the character based on slope/elevation:

| Slope    | Flat terrain char | Steep terrain char |
|----------|------------------|--------------------|
| < 5%     | `·` or `'`       | (same)             |
| 5-15%    | `,` or `~`       | `/` or `\`         |
| 15-30%   | `∧` or `^`       | `▲` or `△`         |
| > 30%    | `#` or `█`       | `▓` or `║`         |

This makes terrain *readable* — you can see the slope at a glance.

#### D. Multiple Point Light Sources
Add a `Vec<PointLight>` to the lighting system. Each point light has position, color, radius, intensity. Additive blend with sun/moon light.

Use cases: campfire glow (orange), smithy (red-orange), moonwell (blue), building windows at night (warm yellow), burning terrain (flickering red).

#### E. Specular Highlights on Water
Water already gets flat normals `(0,0,1)`. Add a slight per-cell wave normal offset (sinusoidal, time-varying) so specular highlights dance across water surfaces:

```rust
let wave_nx = (tick * 0.1 + wx * 0.3).sin() * 0.15;
let wave_ny = (tick * 0.07 + wy * 0.4).cos() * 0.15;
let water_normal = normalize(wave_nx, wave_ny, 1.0);
```

---

## 5. Terrain Color Theory for Visualization

### Elevation Color Ramps

The cartographic standard (hypsometric tinting):
- **Deep water:** Dark blue `(20, 40, 100)` — we have this
- **Shallow water:** Lighter blue-green `(60, 110, 180)`
- **Coastal/sand:** Warm tan `(190, 165, 90)` — we have this
- **Lowland grass:** Rich green `(45, 140, 45)` — we have this
- **Upland/scrub:** Yellow-green to brown `(130, 120, 60)` — we have this
- **Mountain rock:** Gray-brown `(120, 110, 100)` — we have this
- **Alpine/snow:** White-blue `(220, 220, 240)` — we have this

Our palette already follows this convention well. The main improvement would be **continuous interpolation** within biomes based on actual elevation, rather than discrete terrain-type colors.

### Moisture Visualization

- **Arid:** Warm hues (yellow, orange, tan)
- **Moderate:** Neutral greens
- **Wet:** Cool hues (blue-green, teal)
- **Saturated:** Deep blue-greens with darker tones

### Biome-Specific Character Vocabularies

| Biome      | Primary chars | Texture feel           |
|------------|--------------|------------------------|
| Ocean      | `~≈∼`        | Flowing, animated      |
| Beach      | `·.:`        | Dotted, granular       |
| Grassland  | `'",`        | Light, airy            |
| Forest     | `↑♠♣τ`       | Vertical, dense        |
| Desert     | `.:·`        | Sparse, empty          |
| Mountain   | `^▲∧`        | Pointed, angular       |
| Snow       | `·∘°`        | Soft, uniform          |
| Marsh      | `~,;`        | Wet, tangled           |
| Tundra     | `-_≡`        | Flat, barren           |
| River      | `≈~∽`        | Flowing, directional   |
| Road       | `═─│`        | Box drawing, connected |

Sources:
- [9 Creative Color Schemes for Elevation Data — Map Library](https://www.maplibrary.org/1508/creative-color-schemes-for-elevation-data/)
- [Topographic Colormaps — Carl Cervone](https://medium.com/@carlcervone/topographic-colormaps-a565602dd1c6)
- [Heightfield Shading with Blinn-Phong — Nils Olovsson](https://nils-olovsson.se/articles/heightfield_shading/)

---

## 6. Weather, Water, and Dynamic Effects

### Water Rendering (Current + Improvements)

**Current:** Animated `~≈∼` with sinusoidal blue shimmer. Water depth modulates color intensity. This is already good.

**Improvements:**
- **Flow direction arrows:** On rivers, modulate the character based on flow direction using `→←↑↓↗↘↙↖` or simpler directional characters.
- **Depth-based character:** Shallow `~`, medium `≈`, deep `█` (solid blue).
- **Shore foam:** Where water meets land, use lighter blue/white for a foam effect.
- **Frozen transition:** In winter, water gradually transitions from `~` to `=` to `═` as it freezes (we already have Ice terrain).

### Rain Effects

Overlay semi-transparent rain characters on the terrain during rain:
- Light rain: Sparse `·` falling (every Nth cell, position shifts down each tick)
- Heavy rain: Dense `│` or `|` characters
- Implementation: During render, probabilistically replace some cells' foreground char with a rain char, tinted slightly blue. Use `(hash(x, y, tick) % threshold)` for spatial coherence.

### Snow Effects

Similar to rain but slower, using `*` or `·`, accumulating as terrain color shifts toward white.

### Fire Effects

We have `Burning` terrain with `*` and orange color. Improvements:
- Flicker: Randomly alternate between `*`, `+`, `×` each tick
- Color dance: Vary between orange, red, yellow per cell per tick
- Smoke above fire: Gray `·` or `°` characters one row above burning tiles

### Day/Night Transition

**Current:** Ambient tint shifts from warm→blue→silver. Already strong.

**Improvements:**
- **Golden hour effect:** During sunrise/sunset (0 < sun_elev < 0.3), add warm orange to specular highlights. Long shadows are already handled by the sweep.
- **Star field:** At deep night, occasionally render `·` or `*` in dark sky areas (if we ever show sky).
- **Building windows:** At night, building tiles glow warm yellow (point light from buildings).

---

## 7. Rust Terminal Libraries

### Crossterm (Current)

We use crossterm directly for raw terminal access. This is fine and fast.

### Ratatui

Built on crossterm, provides widget abstractions, constraint-based layout, and double-buffering (exactly what we already built manually). Ratatui's buffer diff approach matches our `back`/`front` buffer system.

**Consider for:** If the UI panel grows complex (tabs, scrolling lists, charts), ratatui's widget system could replace our manual panel drawing. Not needed for the game viewport — our custom renderer is more appropriate there.

Source: [Ratatui GitHub](https://github.com/ratatui/ratatui)

### Notcurses

A C library with Rust bindings. Supports:
- **Sixel graphics** — actual bitmap images in terminal cells (requires Sixel-capable terminal)
- **Kitty graphics protocol** — pixel-level image rendering
- **Multiple blitter backends** — falls back gracefully from Sixel → braille → half-block → ASCII

**Consider for:** A future "high-fidelity" mode where terminals that support Sixel/Kitty get actual terrain texture rendering, while others get ASCII. This would be a significant undertaking but the results are dramatic.

Source: [Notcurses GitHub](https://github.com/dankamongmen/notcurses)

---

## 8. Practical Implementation Priorities

Ranked by impact-to-effort ratio:

### Quick Wins (< 1 day each)

1. **Per-cell color noise** — Hash-based RGB variance on terrain colors. Breaks up flat terrain immediately. Near-zero performance cost.

2. **Glyph animation for vegetation** — Forest/grass chars cycle slowly between `'` and `"` and `,` based on position hash + tick. Gives the world a breathing feel.

3. **Fire flicker** — Randomize Burning char and color per tick. Three lines of code, big visual impact.

4. **Fog of war gradient** — Use `░▒▓` for partially-explored areas instead of binary revealed/hidden. Explored-but-not-visible = dim `░`, near edge of vision = `▒`.

5. **Rain overlay** — Probabilistic character replacement during rain seasons. Tick-based downward motion.

### Medium Effort (1-3 days each)

6. **Point light sources** — Campfire/building lights with radial falloff. Additive blend with existing light_map. Major atmosphere boost at night.

7. **Water specular animation** — Sinusoidal normal perturbation on water tiles. Dancing light reflections.

8. **Slope-based glyph selection** — Use terrain normal steepness to pick between flat/steep character variants. Makes topography readable without any overlay.

9. **Half-block minimap** — Overview map using `▄` half-blocks for 2x resolution. Each cell = 2 terrain tiles stacked.

10. **UI box drawing borders** — Replace `-` separators with `─` and add `│` side borders. Use `╭╮╰╯` rounded corners. Small touch, professional feel.

### Larger Projects (3+ days)

11. **Ambient occlusion** — Darken valleys and enclosed spaces. Requires neighbor height sampling.

12. **Connected road/river rendering** — Use box-drawing characters `═║╔╗` etc. to render roads and rivers as connected paths instead of isolated characters.

13. **Building interior rendering** — When zoomed in, show internal structure of buildings using box-drawing walls, floor patterns.

14. **CRT post-processing toggle** — Scan lines + vignette effect (darken edges of viewport). Purely aesthetic, Caves of Qud-inspired.

15. **Notcurses/Sixel backend** — Alternative renderer for high-fidelity terminals. Major project.

---

## 9. Key Tools

- **REXPaint** — Free ASCII art editor by Grid Sage Games. Design UI mockups, test color palettes, create splash screens. Exports to `.xp` format with layer support. [Download](https://kyzrati.itch.io/rexpaint)
- **Lospec** — Browse pixel art palettes. Search for "terminal", "retro", "earthy". [lospec.com](https://lospec.com/palette-list)
- **True color test:** `printf "\x1b[38;2;255;100;0mTRUE COLOR\x1b[0m\n"` — verify terminal supports 24-bit color.

---

## 10. Design Principles (Synthesis)

1. **Color conveys information, not decoration.** Every color choice should help the player read the terrain. Elevation = brightness. Moisture = hue warmth/coolness. Biome = character + base color.

2. **Contrast creates hierarchy.** Terrain is muted. Entities are saturated. Alerts are bright. UI is subdued. This is the Cogmind principle.

3. **Animation is spice, not the meal.** Water shimmer, vegetation sway, fire flicker — use sparingly. Too much movement is distracting and increases terminal bandwidth.

4. **The player's imagination fills gaps.** Stone Story RPG's core insight. A consistent visual language (`~` = water, `^` = mountain) lets players construct rich mental models from minimal glyphs.

5. **Noise breaks monotony.** Per-cell color variance, character cycling, subtle brightness jitter — these prevent large terrain areas from looking like flat blocks of color.

6. **Lighting tells the story of time.** Our day/night system is already strong. Lean into it: golden hour warmth, moonlit blue-silver, shadow sweep drama. This is the single biggest differentiator from other ASCII games.

7. **Test on dark terminals.** Most players use dark backgrounds. Design for dark-on-dark contrast. Our `(25,25,40)` panel background is good. Ensure terrain bg colors never clash with the terminal's own background.
