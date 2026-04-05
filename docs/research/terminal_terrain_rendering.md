# Terminal Terrain Rendering: Deep Technical Reference

**Date:** 2026-04-04
**Purpose:** Comprehensive catalog of terminal rendering techniques for terrain, maps, and natural landscapes. Focuses on techniques beyond what `terminal_visual_design.md` already covers.
**Context:** terrain-gen-rust already has Blinn-Phong lighting, shadow sweep, day/night cycle, seasonal tinting, 24-bit true color via crossterm, dirty-rect rendering, and animated water. This document goes deeper on each axis.

---

## Table of Contents

1. [Sub-Cell Resolution Rendering](#1-sub-cell-resolution-rendering)
2. [Heightmap Visualization in Terminals](#2-heightmap-visualization-in-terminals)
3. [Water Rendering Techniques](#3-water-rendering-techniques)
4. [Weather and Atmosphere](#4-weather-and-atmosphere)
5. [Advanced Lighting for Terminals](#5-advanced-lighting-for-terminals)
6. [Color Science for Terrain Gradients](#6-color-science-for-terrain-gradients)
7. [Dithering and Anti-Banding](#7-dithering-and-anti-banding)
8. [Character Selection Science](#8-character-selection-science)
9. [Minimap and Compressed Views](#9-minimap-and-compressed-views)
10. [Notable Projects and Tools](#10-notable-projects-and-tools)
11. [Demoscene and Terminal Art Techniques](#11-demoscene-and-terminal-art-techniques)
12. [Fog, Depth Cues, and Atmospheric Perspective](#12-fog-depth-cues-and-atmospheric-perspective)
13. [Implementation Priority Matrix](#13-implementation-priority-matrix)

---

## 1. Sub-Cell Resolution Rendering

The single biggest visual upgrade available to any terminal renderer is increasing effective resolution beyond one-pixel-per-cell. There are five tiers of sub-cell rendering, each with different tradeoffs.

### 1.1 Half-Block Rendering (2x1 per cell) -- THE ESSENTIAL TECHNIQUE

**Characters:** `U+2580` (upper half block) or `U+2584` (lower half block)
**Resolution:** 2 vertical pixels per cell. An 80x50 terminal becomes 80x100 effective pixels.
**Color:** Full 24-bit color per sub-pixel -- foreground colors the block half, background colors the other half.

**How it works:**
```
For each pair of vertical pixels (top, bottom):
  Set background color = top pixel color
  Set foreground color = bottom pixel color
  Print '▄' (lower half block)
```

This is the technique used by Notcurses' default NCBLIT_2x1 blitter, pixterm, and ratatui-image's halfblock fallback. It is universally supported across modern terminals.

**Terrain application:** A dedicated minimap mode or overview zoom level using half-blocks would immediately double the apparent detail. Each cell encodes two terrain tiles vertically, with the actual biome color for each.

**Rating: 10/10** -- Trivial to implement, universal support, doubles resolution. Already noted in terminal_visual_design.md but not yet implemented.

Sources:
- [Pixterm -- true color pixel art in terminal](https://github.com/eliukblau/pixterm)
- [Half-height console graphics](https://a.skh.am/2020/11/26/half-height-console-graphics.html)
- [Terminal pixel art -- Lucamug](https://lucamug.medium.com/terminal-pixel-art-ad386d186dad)

### 1.2 Quadrant Block Rendering (2x2 per cell)

**Characters:** `U+2596`-`U+259F` (quadrant blocks: `▖▗▘▙▚▛▜▝▞▟`) plus `U+2580` (upper half), `U+2584` (lower half), `U+258C` (left half), `U+2590` (right half), `U+2588` (full block), and space.
**Resolution:** 4 sub-pixels per cell (2 wide x 2 tall).
**Color limit:** Only 2 colors per cell (fg + bg), so the 4 sub-pixels are binary: each is either fg or bg.

**How it works:**
Given 4 pixel colors in a 2x2 grid, the algorithm:
1. Clusters the 4 colors into 2 groups (minimize max color distance)
2. Assigns fg = average of group A, bg = average of group B
3. Selects the quadrant character whose filled/empty pattern matches the grouping

**The key insight:** With only 2 colors per cell, you must quantize 4 potentially different colors down to 2. The img2unicode project solves this by minimizing total squared error:
```
argmin_{fg, bg, mask} sum_pixels |mask[p] * fg + (1-mask[p]) * bg - actual[p]|^2
```

**Terrain application:** For the minimap, quadrant blocks give 2x horizontal AND 2x vertical resolution, effectively 4x the detail of standard rendering. A 40x25 minimap area becomes 80x50 effective pixels. The 2-color-per-cell limitation is acceptable for terrain where adjacent tiles are usually similar colors.

**Rating: 8/10** -- Significant resolution boost. The 2-color limit means biome boundaries look slightly quantized, but for overview/minimap this is excellent.

Sources:
- [img2unicode -- optimal character selection](https://github.com/matrach/img2unicode)
- [Notcurses NCBLIT_2x2 documentation](https://notcurses.com/notcurses_visual.3.html)
- [Unicode Block Elements](https://www.unicode.org/charts/PDF/U2580.pdf)

### 1.3 Sextant Rendering (2x3 per cell) -- HIGHEST QUALITY CHARACTER BLITTER

**Characters:** `U+1FB00`-`U+1FB3B` (64 sextant characters from Unicode 13 "Symbols for Legacy Computing")
**Resolution:** 6 sub-pixels per cell (2 wide x 3 tall).
**Color limit:** Still 2 colors per cell (fg + bg).

**How it works:**
Each sextant character represents one of 64 possible fill patterns in a 2x3 grid. Combined with the existing half-blocks and full/empty characters, you get every possible binary combination. The algorithm is the same as quadrants but with a 2x3 grid instead of 2x2.

**Font support concern:** Sextant characters are newer (Unicode 13, 2020) and not all terminal fonts render them correctly. Alacritty and some other terminals have had rendering issues with sextant glyphs being misaligned or sized incorrectly. However, foot, kitty, and wezterm handle them well.

**Terrain application:** Sextant blitting for a high-detail minimap would look remarkably close to actual pixel art. At 2x3 sub-pixels per cell, a 60x30 viewport becomes 120x90 effective pixels -- enough to show real terrain features like rivers and coastlines with clarity.

**Rating: 7/10** -- Best character-mode quality, but font support is not universal. Should be offered as an option with quadrant fallback.

Sources:
- [Unicode 13 sextants (HN discussion)](https://news.ycombinator.com/item?id=24956014)
- [Symbols for Legacy Computing (Unicode chart)](https://www.unicode.org/charts/PDF/U1FB00.pdf)
- [Notcurses NCBLIT_3x2 sextant blitter](https://nick-black.com/dankwiki/index.php/Notcurses)

### 1.4 Braille Dot Rendering (2x4 per cell) -- MAXIMUM RESOLUTION

**Characters:** `U+2800`-`U+28FF` (256 braille patterns)
**Resolution:** 8 sub-pixels per cell (2 wide x 4 tall).
**Color limit:** 2 colors per cell, but more critically -- dots are tiny points, not filled rectangles. The visual effect is "pointillist" rather than "pixel art."

**Bit encoding:**
```
Dot positions in a braille cell:
  (0,0)=bit0  (1,0)=bit3
  (0,1)=bit1  (1,1)=bit4
  (0,2)=bit2  (1,2)=bit5
  (0,3)=bit6  (1,3)=bit7

char = '\u{2800}' as u32 + bitfield
```

**Terrain application:** Best for overlay data, not primary terrain rendering. The dot-matrix appearance is too sparse for filled terrain. Excellent for:
- Contour lines overlaid on terrain
- Flow direction vectors
- Wind patterns
- Elevation isolines
- Any data where you want lines/curves at high resolution

MapSCII uses braille as its default blitter for world maps -- the line-drawing nature of geographic boundaries suits braille well.

**Rating: 6/10 for terrain, 9/10 for overlays** -- Not suitable as primary terrain renderer, but unmatched for contour/vector overlays.

Sources:
- [Drawille -- braille pixel graphics in terminal](https://github.com/asciimoo/drawille)
- [MapSCII -- braille world map renderer](https://github.com/rastapasta/mapscii)
- [Unicode graphics overview -- Dernocua](https://dernocua.github.io/notes/unicode-graphics.html)

### 1.5 Diagonal and Wedge Characters -- SLOPE RENDERING

**Characters:** `U+1FB3C`-`U+1FB6F` (diagonal block elements from Legacy Computing)
**These include:** Lower-left diagonal, lower-right diagonal, upper-left diagonal, upper-right diagonal -- each at multiple angle points (1/8, 1/4, 3/8, 1/2, 5/8, 3/4, 7/8 of the cell).

**Terrain application:** These are potentially transformative for slope rendering. Instead of using `^` or `/` to indicate a cliff face, you could use actual diagonal fills that visually approximate the slope angle. A steep cliff would use a near-vertical diagonal; a gentle slope would use a shallow one.

Example cliff rendering:
```
Standard:    Diagonal blocks:
  ^^##        ◸██
  /##/        ◿██◹
  ..//        ··◿◹
```

**Font support concern:** Even worse than sextants -- very few fonts render these well as of 2025. This is bleeding-edge.

**Rating: 5/10** -- Conceptually powerful for terrain slopes but impractical until font support improves. Worth revisiting in 2027+.

Sources:
- [Symbols for Legacy Computing -- Wikipedia](https://en.wikipedia.org/wiki/Symbols_for_Legacy_Computing)
- [Unicode Legacy Computing block chart](https://www.unicode.org/charts/PDF/U1FB00.pdf)

### 1.6 Resolution Comparison Table

| Technique      | Chars/cell | Colors/cell | Effective px (80x50 term) | Font support |
|---------------|-----------|------------|--------------------------|-------------|
| Standard      | 1         | 2 (fg+bg)  | 80 x 50 = 4,000         | Universal   |
| Half-block    | 1         | 2          | 80 x 100 = 8,000        | Universal   |
| Quadrant      | 1         | 2          | 160 x 100 = 16,000      | Very good   |
| Sextant       | 1         | 2          | 160 x 150 = 24,000      | Good        |
| Braille       | 1         | 2          | 160 x 200 = 32,000      | Good        |
| Sixel/Kitty   | N/A       | Full       | Actual pixels            | Limited     |

---

## 2. Heightmap Visualization in Terminals

### 2.1 Contour Lines with Braille

Draw ISO-elevation contour lines using braille characters overlaid on terrain. The algorithm:

1. For each cell, sample elevation at the 8 braille dot positions (interpolated from the heightmap)
2. For each contour level (e.g., every 50m), check which dots are above vs. below
3. Set dots that are near the contour boundary (within a threshold)
4. Combine all contour dots into a single braille character per cell
5. Render the braille character in a contrasting color over the terrain background

This gives smooth, curved contour lines at 2x4 sub-pixel resolution -- far better than any character-based approach.

**Implementation sketch:**
```rust
fn contour_braille(elevations: &[f64; 8], contour_level: f64, threshold: f64) -> char {
    let mut bits: u8 = 0;
    for (i, &elev) in elevations.iter().enumerate() {
        if (elev - contour_level).abs() < threshold {
            bits |= 1 << BRAILLE_BIT_ORDER[i];
        }
    }
    char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
}
```

**Rating: 9/10** -- Would produce beautiful topographic map overlays. Low cost, high visual impact.

### 2.2 Hillshading in Terminal

We already have Blinn-Phong lighting that effectively IS hillshading. But there are cartographic hillshading techniques that complement it:

**Multi-directional hillshading (MDOW):** Instead of a single light direction, combine shading from multiple azimuths (typically 6: every 60 degrees). Average the results. This eliminates the "shadow bias" where slopes facing away from the single light source appear uniformly dark, losing detail. Swiss cartographers use this technique.

**Implementation:** We already compute normals. For MDOW:
```rust
let azimuths = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0];
let combined = azimuths.iter()
    .map(|az| compute_hillshade(normal, sun_elevation, *az))
    .sum::<f64>() / 6.0;
```

This could be a toggle mode ("cartographic view") that sacrifices time-of-day lighting realism for maximum terrain readability.

**Rating: 6/10** -- We already have superior single-source lighting. MDOW would be useful as a debug/cartographic overlay but less atmospheric for gameplay.

### 2.3 Hypsometric Tinting with Continuous Elevation

Instead of discrete terrain-type colors, interpolate color continuously based on elevation:

```
elevation 0-50m:   dark_green -> light_green
elevation 50-200m: light_green -> yellow_green  
elevation 200-500m: yellow_green -> brown
elevation 500-1000m: brown -> gray
elevation 1000m+:  gray -> white
```

The key insight: interpolate in OKLab color space (see Section 6) for perceptually uniform gradients that avoid the muddy middle tones you get from RGB interpolation.

**Rating: 8/10** -- Would make elevation immediately readable through color alone, complementing the character-based terrain type display.

### 2.4 Slope Visualization via Character Density

Map slope steepness to character visual density:

| Slope      | Character | Visual density | Meaning              |
|-----------|-----------|---------------|----------------------|
| 0-5%      | `·`       | ~5%           | Flat plain           |
| 5-15%     | `'`       | ~10%          | Gentle slope         |
| 15-30%    | `:`       | ~20%          | Moderate hill        |
| 30-50%    | `%`       | ~40%          | Steep hillside       |
| 50-80%    | `#`       | ~60%          | Very steep           |
| 80%+      | `█`       | ~100%         | Cliff face           |

This works because the human eye naturally reads denser patterns as "heavier" or "steeper." Combined with lighting, you get both slope direction (from light/shadow) and slope magnitude (from character density).

**Rating: 7/10** -- Good for readability but conflicts with terrain-type character assignments. Best as an optional overlay.

---

## 3. Water Rendering Techniques

### 3.1 Flow-Directional Characters

Replace uniform `~` water tiles with characters that indicate flow direction:

```rust
fn flow_char(flow_dx: f64, flow_dy: f64, tick: u64) -> char {
    let angle = flow_dy.atan2(flow_dx);
    let phase = (tick % 3) as usize;
    // 8 directions x 3 animation frames
    match (((angle + PI) / (PI / 4.0)) as usize % 8, phase) {
        (0, 0) => '~', (0, 1) => '≈', (0, 2) => '~',  // East
        (1, 0) => '/', (1, 1) => '╱', (1, 2) => '/',    // NE
        (2, 0) => '|', (2, 1) => '│', (2, 2) => '¦',    // North
        (3, 0) => '\\', (3, 1) => '╲', (3, 2) => '\\',  // NW
        (4, 0) => '~', (4, 1) => '≈', (4, 2) => '~',    // West
        (5, 0) => '/', (5, 1) => '╱', (5, 2) => '/',    // SW
        (6, 0) => '|', (6, 1) => '│', (6, 2) => '¦',    // South
        (7, 0) => '\\', (7, 1) => '╲', (7, 2) => '\\',  // SE
        _ => '~',
    }
}
```

Rivers immediately become readable -- you can see which way water flows without any overlay. The phase animation gives the illusion of motion.

**Rating: 8/10** -- High impact for rivers. We already have flow data from hydrology; this just visualizes it.

### 3.2 Depth-Stratified Water Characters

Different characters for different water depths:

| Depth      | Char  | Color               | Effect                    |
|-----------|-------|---------------------|---------------------------|
| < 0.5m   | `·`   | Light blue-green    | Puddle / shallow ford     |
| 0.5-2m   | `~`   | Medium blue         | Stream / shallow water    |
| 2-5m     | `≈`   | Darker blue         | River / lake edge         |
| 5-20m    | `∼`   | Deep blue           | Deep river / lake         |
| > 20m    | `█`   | Very dark blue      | Ocean / deep lake         |

The transition from sparse characters (dots) to dense characters (full block) creates a natural "looking into deeper water" effect.

**Rating: 7/10** -- We partially have this. Full implementation would add real depth perception to water bodies.

### 3.3 Specular Caustics on Water

Caustics are the dancing light patterns on the bottom of shallow water. In a terminal, simulate this by:

1. Compute a Voronoi noise pattern that shifts with time
2. Where the Voronoi cell edges intersect with water tiles, brighten the water color
3. Animate by offsetting the noise seed each tick

```rust
fn water_caustic_brightness(wx: usize, wy: usize, tick: u64) -> f64 {
    // Cheap Voronoi approximation: distance to nearest hash point
    let t = tick as f64 * 0.05;
    let px = wx as f64 + t.sin() * 0.5;
    let py = wy as f64 + (t * 0.7).cos() * 0.5;
    
    // Hash-based nearest point
    let cell_x = px.floor() as i32;
    let cell_y = py.floor() as i32;
    let mut min_dist = 10.0_f64;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let hash = simple_hash(cell_x + dx, cell_y + dy);
            let point_x = (cell_x + dx) as f64 + (hash % 100) as f64 / 100.0;
            let point_y = (cell_y + dy) as f64 + ((hash / 100) % 100) as f64 / 100.0;
            let dist = ((px - point_x).powi(2) + (py - point_y).powi(2)).sqrt();
            min_dist = min_dist.min(dist);
        }
    }
    // Caustic = bright at cell edges (where min_dist is near 0.5)
    let caustic = (1.0 - (min_dist - 0.5).abs() * 4.0).max(0.0);
    caustic * 0.3 // Subtle 0-30% brightness boost
}
```

Apply as a brightness multiplier on shallow water. The effect is subtle but reads as "sunlight through water" -- very atmospheric.

**Rating: 7/10** -- Adds real atmosphere to water. Moderate implementation cost. Only visible during daytime on shallow water.

### 3.4 Shore Foam / Surf Line

Where water meets land, add a white/light-blue foam effect:

```rust
if terrain_is_water && any_neighbor_is_land {
    // Foam: lighter color, animated character
    let foam_chars = ['·', '°', '∘', '·'];
    let phase = (tick / 6 + wx + wy * 3) % 4;
    fg = Color(200, 220, 240); // Near-white
    ch = foam_chars[phase as usize];
}
```

This creates a visible coastline that "breathes" with wave-like animation. The changing phase makes it look like waves lapping at the shore.

**Rating: 8/10** -- Very cheap, immediately makes coastlines look alive. High visual payoff.

### 3.5 Frozen Water Transition

Progressive freezing animation across seasons:

| Season phase | Characters | Color shift       | Effect              |
|-------------|-----------|-------------------|---------------------|
| Late autumn | `~≈~`     | Blue -> blue-gray | Water cooling       |
| Early winter| `~=~`     | Blue-gray         | Partial freeze      |
| Mid winter  | `═══`     | White-blue        | Solid ice           |
| Late winter | `=~=`     | Blue-gray         | Thawing edges       |
| Spring      | `~≈~`     | Blue              | Fully liquid        |

The character transition from wavy to rigid (tilde to equals to box-drawing) visually communicates the phase change.

**Rating: 6/10** -- We already have Ice terrain. This just adds the transition animation.

---

## 4. Weather and Atmosphere

### 4.1 Rain Rendering: The Overlay Approach

Rain is best implemented as a post-processing overlay on the final rendered frame, not as part of terrain rendering. This keeps the rain system completely decoupled.

**Algorithm:**
```rust
fn apply_rain_overlay(frame: &mut FrameBuffer, tick: u64, intensity: f64) {
    let threshold = (1.0 - intensity) * 100.0; // 0% = all rain, 100% = no rain
    for y in 0..frame.height {
        for x in 0..frame.width {
            // Deterministic "random" per cell per tick
            let hash = fast_hash(x, y + tick * 3) % 100;  // drops fall down
            if hash as f64 > threshold {
                let drop_type = hash % 3;
                match drop_type {
                    0 => {
                        // Light rain: single dot
                        frame.fg[y][x] = blend(frame.fg[y][x], Color(150, 170, 200), 0.4);
                        frame.ch[y][x] = '·';
                    }
                    1 => {
                        // Medium rain: vertical bar
                        frame.fg[y][x] = blend(frame.fg[y][x], Color(130, 150, 190), 0.5);
                        frame.ch[y][x] = '│';
                    }
                    _ => {
                        // Heavy rain: full streak
                        frame.fg[y][x] = blend(frame.fg[y][x], Color(110, 130, 180), 0.6);
                        frame.ch[y][x] = '|';
                    }
                }
            }
        }
    }
}
```

**Key insight:** The `y + tick * 3` offset in the hash makes drops appear to fall downward at 3 cells per tick. By shifting the y-coordinate with time, the same deterministic hash produces the illusion of downward motion without any per-drop state.

**Wind integration:** Replace `y + tick * 3` with `y + tick * 3 + x * wind_factor` to make rain fall at an angle. Positive wind_factor = rain blown right, negative = blown left.

**Rating: 9/10** -- Huge atmospheric impact, trivial to implement, completely decoupled from terrain rendering. The WeatherSpect project proves this approach works beautifully.

Sources:
- [WeatherSpect -- ASCII weather simulation](https://github.com/AnotherFoxGuy/weatherspect)
- [Terminal Rain-Lightning](https://github.com/rmaake1/terminal-rain-lightning)
- [Weathr -- terminal weather app with animations](https://github.com/veirt/weathr)

### 4.2 Snow Rendering

Snow is slower, sparser, and drifts more horizontally than rain:

```rust
// Snow falls slowly (tick / 4 instead of tick * 3)
// Drifts sideways (larger x contribution)
let hash = fast_hash(x + tick / 2, y + tick / 4) % 200;
if hash as f64 > threshold {
    let snow_chars = ['*', '·', '°', '∘'];
    frame.ch[y][x] = snow_chars[(hash / 50) as usize];
    frame.fg[y][x] = Color(220, 225, 235); // Slightly blue-white
}
```

**Accumulation effect:** After sustained snowfall, gradually shift terrain background colors toward white. This creates visible snow cover that builds up over time:
```rust
let snow_depth = weather.accumulated_snow; // 0.0 to 1.0
terrain_bg = lerp_oklab(terrain_bg, Color(230, 235, 245), snow_depth * 0.6);
```

**Rating: 8/10** -- Seasonal snow accumulation would be visually spectacular. The particle overlay is cheap; the accumulation requires tracking per-tile snow depth.

### 4.3 Fog / Mist as Distance Attenuation

Terminal fog uses the same principle as 3D distance fog: blend cell colors toward a fog color based on distance from camera or elevation.

**Height fog (mist in valleys):**
```rust
let fog_density = (valley_threshold - elevation).max(0.0) / valley_threshold;
let fog_color = Color(180, 185, 195); // Cool gray-blue
fg = lerp_oklab(fg, fog_color, fog_density * 0.7);
bg = lerp_oklab(bg, fog_color, fog_density * 0.7);
// Also reduce character contrast in fog
if fog_density > 0.5 {
    ch = ' '; // In thick fog, terrain characters disappear
}
```

**Key insight:** Fog should also affect the CHARACTER, not just color. In thick fog, terrain features vanish -- replace detailed characters with spaces or dots. This is how real fog works: you lose shape before you lose color.

**Distance fog (haze at map edges):**
```rust
let dist = distance_from_camera_center(wx, wy);
let haze = ((dist - clear_radius) / fade_distance).clamp(0.0, 1.0);
fg = lerp(fg, haze_color, haze * 0.5);
```

**Rating: 8/10** -- Height fog in valleys would add massive depth to the terrain. Distance haze is a nice subtle touch. Both are cheap to compute.

### 4.4 Lightning Flash

A full-screen brightness flash that decays over 3-5 ticks:

```rust
if lightning_timer > 0 {
    let flash = (lightning_timer as f64 / 5.0).powi(2); // Quadratic decay
    fg = lighten(fg, flash * 0.8);
    bg = lighten(bg, flash * 0.6);
    lightning_timer -= 1;
}
```

Trigger randomly during thunderstorms. The quadratic decay gives a sharp flash that fades naturally.

For extra effect: on the first tick, also draw a lightning bolt using box-drawing characters (`│╲╱─`) from a random point at the top of the screen to a random point lower down. Clear it on tick 2.

**Rating: 7/10** -- Simple, dramatic. Only useful during storms but very immersive when it hits.

### 4.5 Cloud Shadows

Slowly moving dark patches across the terrain that represent cloud shadows:

```rust
fn cloud_shadow(wx: usize, wy: usize, tick: u64) -> f64 {
    // Large-scale Perlin noise, slowly scrolling
    let cloud_x = wx as f64 * 0.02 + tick as f64 * 0.001;
    let cloud_y = wy as f64 * 0.02 + tick as f64 * 0.0003;
    let cloud_val = perlin_2d(cloud_x, cloud_y);
    
    // Threshold to create patches, not gradients
    if cloud_val > 0.3 {
        0.7 // Under cloud: 30% darker
    } else {
        1.0 // Clear sky: full brightness
    }
}
```

Apply as a multiplier to terrain brightness. The slowly scrolling noise creates the impression of clouds drifting overhead.

**Rating: 9/10** -- Very atmospheric, almost zero cost (noise is cheap), and creates beautiful rolling shadow patterns across the landscape. This is one of those "why didn't we think of this" techniques.

---

## 5. Advanced Lighting for Terminals

### 5.1 Per-Side Wall Lighting (Gridbugs/Brogue Technique)

The most sophisticated lighting technique in terminal roguelikes tracks light reaching each SIDE of each cell, not just the cell center. This is how Brogue achieves its signature look.

**The problem it solves:** A wall between a torch and the player should glow orange on the torch-facing side and remain dark on the player-facing side. If you only track per-cell brightness, the whole wall is either lit or dark.

**Implementation:**
```rust
struct CellLighting {
    north: Color,   // Light hitting the north face
    south: Color,   // Light hitting the south face  
    east: Color,    // Light hitting the east face
    west: Color,    // Light hitting the west face
    corners: [Color; 4], // NW, NE, SW, SE corner illumination
}

// When rendering a wall cell, pick the side facing the camera
// or the side with the most visible light
fn wall_render_color(lighting: &CellLighting, visible_sides: &[bool; 4]) -> Color {
    visible_sides.iter()
        .zip([lighting.north, lighting.south, lighting.east, lighting.west])
        .filter(|(&visible, _)| visible)
        .map(|(_, color)| color)
        .max_by_key(|c| c.luminance())
        .unwrap_or(Color(0, 0, 0))
}
```

**Terrain application:** This is most impactful for building walls and cliff faces. A cliff lit by the setting sun would glow warm on its west face and go dark on its east face. Currently our Blinn-Phong gives directional lighting via normals, but this per-side approach is more physically accurate for vertical features.

**Rating: 7/10** -- Significant visual improvement for buildings and cliffs. Moderate implementation complexity. The existing normal-based system handles terrain well; per-side helps most with vertical structures.

Sources:
- [Gridbugs -- Roguelike Lighting Demo](https://www.gridbugs.org/roguelike-lighting-demo/)
- [Gridbugs -- Another Roguelike Lighting Demo](https://www.gridbugs.org/another-roguelike-lighting-demo/)
- [BrogueCE source code](https://github.com/tmewett/BrogueCE)

### 5.2 Light Channel Filtering (Brogue Technique)

Brogue uses a bitfield-based system where each light source declares which "channels" it illuminates, and each tile declares which channels affect it. This prevents self-illumination artifacts.

**Example:** A lighthouse beam illuminates channel `DISTANT_LIGHT`. Only terrain tiles have this channel. The lighthouse building itself only has `LOCAL_LIGHT`. So the beam doesn't light up the lighthouse interior -- it only lights distant terrain.

**Terrain application:** Useful once we have multiple light sources:
- Sunlight: affects all outdoor terrain
- Firelight: affects nearby tiles only
- Building interior light: leaks through windows but doesn't illuminate building walls from outside

**Rating: 5/10** -- Only relevant once we have multiple light types. Good architecture to plan for.

### 5.3 Ambient Occlusion for Terrain

For a 2D heightmap, ambient occlusion can be cheaply approximated:

```rust
fn terrain_ao(heightmap: &Grid<f64>, x: usize, y: usize) -> f64 {
    let center = heightmap.get(x, y);
    let mut occlusion = 0.0;
    let mut count = 0.0;
    
    for dy in -2..=2_i32 {
        for dx in -2..=2_i32 {
            if dx == 0 && dy == 0 { continue; }
            if let Some(h) = heightmap.get_checked(
                x.wrapping_add(dx as usize), 
                y.wrapping_add(dy as usize)
            ) {
                let height_diff = (h - center).max(0.0);
                let dist = ((dx * dx + dy * dy) as f64).sqrt();
                occlusion += height_diff / dist; // Nearer, taller neighbors occlude more
                count += 1.0;
            }
        }
    }
    
    let ao = (occlusion / count * 3.0).min(0.35); // Max 35% darkening
    1.0 - ao
}
```

**Where it helps most:**
- Valley floors between mountains: naturally darker
- Inside forest clearings: surrounded by tall trees
- River gorges: steep walls on both sides
- At the base of cliffs

This is computed once when terrain generates (or when heightmap changes) and cached. Near-zero runtime cost.

**Rating: 8/10** -- Cheap to precompute, adds significant depth. Valley fog + AO together would make valleys dramatically atmospheric.

### 5.4 Rim Lighting for Entities

Make entities visible against dark terrain by adding a bright outline on the edge facing the light:

```rust
fn entity_rim_light(entity_color: Color, sun_dir: (f64, f64), entity_facing: (f64, f64)) -> Color {
    // Rim light appears when surface faces AWAY from camera (toward light)
    let rim_factor = dot(sun_dir, entity_facing).max(0.0).powf(3.0);
    lighten(entity_color, rim_factor * 0.4)
}
```

In practice for a terminal game, "rim lighting" means making the entity character slightly brighter when backlit -- the entity pops off the terrain more during sunrise/sunset when the sun is low.

**Rating: 4/10** -- Subtle effect, mostly lost at single-character resolution. Nice to have but low priority.

### 5.5 God Rays in Terminal

Crepuscular rays (god rays) can be approximated in a terminal by drawing bright streaks radiating from the sun position through gaps in terrain/clouds:

**Algorithm (screen-space post-process):**
1. Determine the sun's screen position (may be off-screen)
2. For each cell, march from the cell toward the sun position in screen space
3. Count how many cells along the ray are "occluding" (mountains, tall buildings)
4. The fewer occluders, the brighter the ray contribution
5. Add a golden tint proportional to ray brightness

```rust
fn god_ray_brightness(sx: i32, sy: i32, sun_sx: i32, sun_sy: i32, 
                       occlusion_map: &Grid<bool>, samples: usize) -> f64 {
    let mut brightness = 0.0;
    for i in 0..samples {
        let t = i as f64 / samples as f64;
        let sample_x = lerp(sx as f64, sun_sx as f64, t) as usize;
        let sample_y = lerp(sy as f64, sun_sy as f64, t) as usize;
        if !occlusion_map.get(sample_x, sample_y) {
            brightness += 1.0 / samples as f64;
        }
    }
    brightness * 0.15 // Subtle 0-15% brightness addition
}
```

**When to use:** Only during sunrise/sunset (low sun elevation) when mountains create clear silhouettes. The rays would be visible as golden streaks across valley terrain.

**Rating: 6/10** -- Visually impressive but expensive (ray marching per cell) and only applicable during golden hour. Consider as a special-occasion effect.

### 5.6 Campfire / Point Light System

Multiple point lights with color and radial falloff:

```rust
struct PointLight {
    x: f64, y: f64,
    color: Color,
    radius: f64,
    intensity: f64,
    flicker: bool,  // If true, intensity varies per tick
}

fn point_light_contribution(light: &PointLight, wx: f64, wy: f64, tick: u64) -> Color {
    let dist = ((wx - light.x).powi(2) + (wy - light.y).powi(2)).sqrt();
    if dist > light.radius { return Color(0, 0, 0); }
    
    let falloff = 1.0 - (dist / light.radius).powi(2); // Quadratic falloff
    let mut intensity = light.intensity * falloff;
    
    if light.flicker {
        // Hash-based flicker: deterministic but looks random
        let flicker = 0.7 + 0.3 * ((tick as f64 * 0.3 + dist).sin() * 0.5 + 0.5);
        intensity *= flicker;
    }
    
    Color(
        (light.color.0 as f64 * intensity) as u8,
        (light.color.1 as f64 * intensity) as u8,
        (light.color.2 as f64 * intensity) as u8,
    )
}
```

**Additive blending with existing lighting:**
```rust
let sun_light = existing_lightmap[y][x];
let point_sum: Color = point_lights.iter()
    .map(|l| point_light_contribution(l, wx, wy, tick))
    .fold(Color(0,0,0), |a, b| add_colors(a, b));
let final_light = add_colors(sun_light, point_sum).clamp();
```

**Light types for the game:**
| Source           | Color             | Radius | Flicker | When visible       |
|-----------------|-------------------|--------|---------|--------------------|
| Campfire        | (255, 140, 40)    | 6      | Yes     | Always (night best)|
| Smithy forge    | (255, 100, 30)    | 4      | Yes     | When operating     |
| Building window | (240, 200, 120)   | 3      | No      | Night only         |
| Moonwell/spring | (100, 150, 255)   | 5      | No      | Night only         |
| Burning terrain | (255, 80, 20)     | 3      | Yes     | During fire        |
| Biolum. swamp   | (50, 200, 100)    | 4      | Yes     | Night only         |

**Rating: 9/10** -- This is the single highest-impact lighting upgrade. Night scenes with campfire glow would be transformative. Already identified in terminal_visual_design.md but worth detailed specification here.

---

## 6. Color Science for Terrain Gradients

### 6.1 OKLab: The Right Color Space for Terrain

**Why RGB interpolation fails for terrain:** When you linearly interpolate between green (forest) and yellow (grassland) in RGB, the midpoint passes through a desaturated, muddy olive. In OKLab, the midpoint is a vivid yellow-green that looks natural.

**OKLab conversion (sRGB to OKLab):**
```rust
fn srgb_to_oklab(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    // Step 1: sRGB to linear RGB
    let r = srgb_transfer_inv(r as f64 / 255.0);
    let g = srgb_transfer_inv(g as f64 / 255.0);
    let b = srgb_transfer_inv(b as f64 / 255.0);
    
    // Step 2: Linear RGB to LMS (cone response)
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    
    // Step 3: Cube root (perceptual nonlinearity)
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    
    // Step 4: LMS to OKLab
    let L = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;
    
    (L, a, b)
}

fn srgb_transfer_inv(x: f64) -> f64 {
    if x <= 0.04045 { x / 12.92 }
    else { ((x + 0.055) / 1.055).powf(2.4) }
}
```

**Inverse (OKLab to sRGB):**
```rust
fn oklab_to_srgb(L: f64, a: f64, b: f64) -> (u8, u8, u8) {
    let l_ = L + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = L - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = L - 0.0894841775 * a - 1.2914855480 * b;
    
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    
    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    
    (srgb_transfer(r), srgb_transfer(g), srgb_transfer(b))
}

fn srgb_transfer(x: f64) -> u8 {
    let y = if x <= 0.0031308 { 12.92 * x }
            else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (y * 255.0).clamp(0.0, 255.0) as u8
}
```

**Where to use OKLab in terrain-gen-rust:**
1. **Biome transition blending:** When two biome colors meet, interpolate in OKLab
2. **Day/night tinting:** Apply ambient light by adjusting the L channel in OKLab
3. **Elevation gradients:** Continuous hypsometric tinting via OKLab interpolation
4. **Fog blending:** Fade toward fog color in OKLab for natural desaturation

**Rating: 9/10** -- Every color interpolation in the renderer should use OKLab. The formulas are ~20 lines of code and the visual improvement is dramatic. Muddy biome transitions become vibrant.

Sources:
- [OKLab -- Bjorn Ottosson](https://bottosson.github.io/posts/oklab/)
- [OKLCH in CSS -- Evil Martians](https://evilmartians.com/chronicles/oklch-in-css-why-quit-rgb-hsl)
- [Oklab color space -- Wikipedia](https://en.wikipedia.org/wiki/Oklab_color_space)

### 6.2 OKLCH for Hue Rotation (Seasons, Time of Day)

OKLCH is the cylindrical form of OKLab: (L, C, H) where C = chroma (saturation), H = hue angle.

**Seasonal color shifts via hue rotation:**
```rust
fn seasonal_hue_shift(base_oklch: (f64, f64, f64), season: Season) -> (f64, f64, f64) {
    let (l, c, h) = base_oklch;
    match season {
        Season::Spring => (l * 1.05, c * 1.1, h),                    // Brighter, more saturated
        Season::Summer => (l, c, h),                                   // Baseline
        Season::Autumn => (l * 0.95, c * 0.9, (h + 20.0) % 360.0),  // Shift toward warm
        Season::Winter => (l * 0.85, c * 0.5, h),                     // Darker, desaturated
    }
}
```

Rotating hue in OKLCH is perceptually uniform -- a 20-degree shift toward warm doesn't accidentally change perceived brightness, unlike HSV where hue rotation causes dramatic luminance swings.

**Caution:** As noted in recent criticism of OKLCH, gradients in OKLCH can produce out-of-gamut colors. Always clamp to sRGB after conversion. Using OKLab (Cartesian) for interpolation and OKLCH only for hue manipulation avoids most issues.

**Rating: 7/10** -- Useful for seasonal tinting. We already have seasonal color shifts; switching to OKLCH would make them more perceptually uniform.

### 6.3 Anti-Banding via Perceptual Noise

Even with 24-bit color (256 levels per channel), smooth gradients across large terrain areas can show visible banding. The fix: add tiny noise in OKLab space.

```rust
fn debanding_noise(wx: usize, wy: usize) -> f64 {
    // Triangular-PDF noise: -0.5 to +0.5 with triangular distribution
    let h1 = (fast_hash(wx, wy) % 256) as f64 / 256.0;
    let h2 = (fast_hash(wx + 1000, wy + 1000) % 256) as f64 / 256.0;
    (h1 + h2) / 2.0 - 0.5  // Triangular distribution centered at 0
}

// Apply to lightness channel only (most visible axis)
let noise = debanding_noise(wx, wy) * 0.02; // +/- 2% lightness
let (mut l, a, b) = srgb_to_oklab(fg.0, fg.1, fg.2);
l = (l + noise).clamp(0.0, 1.0);
let (r, g, b) = oklab_to_srgb(l, a, b);
```

**Why triangular-PDF noise:** It averages to zero (no brightness bias) and concentrates near zero (less visible than uniform noise). This is the same technique used in professional video debanding.

**Rating: 7/10** -- Only matters for large uniform terrain areas (oceans, plains). When it matters, it is the difference between "flat digital" and "natural."

---

## 7. Dithering and Anti-Banding

### 7.1 Ordered Dithering for Biome Transitions

At biome boundaries, instead of a hard color cutoff, use ordered dithering to create a stippled transition zone:

```rust
// 4x4 Bayer matrix (normalized to 0..1)
const BAYER_4X4: [[f64; 4]; 4] = [
    [ 0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0],
    [12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0],
    [ 3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0],
    [15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0],
];

fn dithered_biome_transition(wx: usize, wy: usize, 
                              biome_a: Color, biome_b: Color, 
                              blend_factor: f64) -> Color {
    let threshold = BAYER_4X4[wy % 4][wx % 4];
    if blend_factor > threshold {
        biome_b
    } else {
        biome_a
    }
}
```

This produces a stippled boundary zone that looks like natural biome mixing -- some grass tiles, some forest tiles, gradually shifting. Much more natural than a hard line or a smooth gradient (which can look artificial at character resolution).

**Rating: 8/10** -- Excellent for biome boundaries. The ordered pattern at 4x4 is invisible at terminal viewing distances.

### 7.2 Temporal Dithering for Animation

When colors need to change smoothly over time (day/night transitions, seasonal shifts), use temporal dithering: alternate between the two nearest colors across frames.

```rust
fn temporal_dither(old_color: Color, new_color: Color, 
                    transition_progress: f64, tick: u64) -> Color {
    // Instead of interpolating every tick (visible stepping),
    // probabilistically choose old or new based on progress
    let threshold = (fast_hash(tick as usize, 0) % 100) as f64 / 100.0;
    if transition_progress > threshold {
        new_color
    } else {
        old_color
    }
}
```

At 10+ FPS, the flicker is invisible and the transition appears smooth even when the two colors differ by only 1-2 RGB levels. This is how displays with 6-bit panels simulate 8-bit color depth.

**Rating: 5/10** -- Our transitions are already smooth enough at 24-bit depth. Only needed if we notice stepping artifacts.

### 7.3 Blue Noise Dithering for Natural Scatter

Blue noise produces evenly-distributed random points with no clumping. Use it for:
- Vegetation placement within biomes
- Snow particle distribution
- Star placement in night sky
- Any scatter pattern that should look "natural"

Blue noise is generated via a precomputed texture (e.g., 64x64 blue noise pattern tiled across the map). Each tile uses its position in the blue noise texture as a threshold for whether to place a feature.

**Rating: 6/10** -- Useful for natural-looking scatter but not critical given our hash-based approach already works reasonably well.

---

## 8. Character Selection Science

### 8.1 Visual Density Matching

The img2unicode project's key insight: match the visual "density" (percentage of filled pixels) of a Unicode character to the brightness/darkness of the terrain feature it represents.

**Character density table (approximate, monospace font):**

| Char | Fill % | Best for                        |
|------|--------|---------------------------------|
| ` `  | 0%     | Sky, deep unexplored            |
| `·`  | ~3%    | Sand, snow, sparse ground       |
| `.`  | ~5%    | Desert, beach, bare rock        |
| `'`  | ~8%    | Short grass, tundra             |
| `,`  | ~8%    | Low vegetation, marsh edge      |
| `:`  | ~10%   | Gravel, rocky ground            |
| `-`  | ~12%   | Flat terrain, ice               |
| `"`  | ~15%   | Medium grass, fields            |
| `~`  | ~15%   | Water surface                   |
| `=`  | ~25%   | Roads, ice, frozen water        |
| `+`  | ~25%   | Crossroads, structures          |
| `*`  | ~30%   | Fire, flowers, special          |
| `o`  | ~35%   | Boulders, entities              |
| `%`  | ~40%   | Rocky terrain, rubble           |
| `#`  | ~50%   | Dense forest, walls             |
| `@`  | ~55%   | Very dense, special entities    |
| `░`  | ~25%   | Light shade, fog of war         |
| `▒`  | ~50%   | Medium shade, partial cover     |
| `▓`  | ~75%   | Dense shade, heavy canopy       |
| `█`  | ~100%  | Solid, deep water, walls        |

**Principle:** Light terrain = sparse characters. Dense terrain = heavy characters. This creates a natural visual texture that maps to the "heaviness" of the landscape feature.

**Rating: 8/10** -- We already do this intuitively. Codifying it as a rule would prevent future regressions and guide new terrain type additions.

### 8.2 Semantic Character Vocabulary

Consistent character meanings across the game:

| Domain      | Chars             | Mnemonic                            |
|------------|-------------------|-------------------------------------|
| Water       | `~≈∼`            | Wavy = liquid                       |
| Vegetation  | `'",τ♣`          | Light strokes = plant-like          |
| Rock/cliff  | `^▲#█`           | Angular, dense = solid              |
| Sand/dirt   | `·.:;`           | Dots = granular particles           |
| Snow/ice    | `°∘·=`           | Light, round = soft cold            |
| Roads       | `═─│╔╗╚╝`        | Box-drawing = constructed paths     |
| Buildings   | `┌┐└┘├┤`         | Box-drawing = built structures      |
| Fire        | `*+×✦`           | Radiating = energy                  |
| Marsh       | `~';,`           | Mixed water + vegetation            |

### 8.3 Aspect-Ratio Aware Character Pairs

Terminal cells are ~2:1 (width:height). For features that should look square (buildings, roads), always use 2 cells wide:

```
Building footprint (2-cell-wide):
  ┌──────┐
  │ ░░░░ │
  │ ░░░░ │
  └──────┘
```

For terrain, the CELL_ASPECT system already handles this by rendering 2 columns per world tile. Key: characters with horizontal orientation (`~`, `-`, `=`) look wider than they are; vertical characters (`|`, `│`) look taller. Choose characters whose visual weight matches the terrain's natural aspect.

### 8.4 The Stipple Gradient for Smooth Biome Edges

Instead of a single character per biome, define a gradient of characters from sparse to dense within each biome. At biome edges, interpolate by selecting earlier characters in the gradient:

```
Forest gradient:  · ' " τ ♣ # ♠
  Forest interior uses ♣ # ♠ (dense)
  Forest edge uses ' " τ (sparse) 
  Grassland-forest border uses · ' (very sparse)
```

This creates natural-looking ecotones (transition zones between biomes) without any color blending.

**Rating: 7/10** -- Adds ecological realism. Implementation is straightforward given per-cell vegetation density data.

---

## 9. Minimap and Compressed Views

### 9.1 Color-Only Minimap (Maximum Information Density)

For the most compressed view, render each minimap cell as a space character with only background color set. No characters = no visual noise. Pure color carries the terrain information.

```rust
fn draw_minimap_cell(renderer: &mut dyn Renderer, sx: u16, sy: u16, terrain: Terrain) {
    let bg = terrain_minimap_color(terrain);
    renderer.draw(sx, sy, ' ', Color(0, 0, 0), Some(bg));
}
```

A 30x20 minimap panel shows 30x20 terrain tiles. Each tile is a single colored square.

**Rating: 7/10** -- Simple and effective. The baseline minimap approach.

### 9.2 Half-Block Minimap (2x Vertical Density)

Use `▄` to pack 2 terrain tiles per cell vertically:

```rust
fn draw_minimap_halfblock(renderer: &mut dyn Renderer, sx: u16, sy: u16, 
                           top_terrain: Terrain, bottom_terrain: Terrain) {
    let bg = terrain_minimap_color(top_terrain);     // Upper pixel
    let fg = terrain_minimap_color(bottom_terrain);   // Lower pixel
    renderer.draw(sx, sy, '▄', fg, Some(bg));
}
```

A 30x20 minimap now shows 30x40 terrain tiles. The same panel space, twice the detail.

**Rating: 9/10** -- The single best minimap technique. Trivial to implement, universal support.

### 9.3 Quadrant Minimap (4x Density)

Use quadrant characters to pack 4 terrain tiles per cell (2x2 grid):

```rust
fn draw_minimap_quadrant(renderer: &mut dyn Renderer, sx: u16, sy: u16,
                          colors: [Color; 4]) -> (char, Color, Color) {
    // Find the best 2-color approximation of 4 terrain colors
    // Cluster colors into 2 groups, then select quadrant character
    let (fg, bg, pattern) = optimal_quadrant_split(&colors);
    let ch = QUADRANT_CHARS[pattern]; // Maps 4-bit pattern to quadrant char
    renderer.draw(sx, sy, ch, fg, Some(bg));
}

const QUADRANT_CHARS: [char; 16] = [
    ' ',  '▘', '▝', '▀',  // 0000, 0001, 0010, 0011
    '▖', '▌', '▞', '▛',  // 0100, 0101, 0110, 0111
    '▗', '▚', '▐', '▜',  // 1000, 1001, 1010, 1011
    '▄', '▙', '▟', '█',  // 1100, 1101, 1110, 1111
];
```

A 30x20 minimap shows 60x40 terrain tiles -- 2400 tiles in 600 cells.

**Problem:** With only 2 colors per cell, you lose detail when 4 tiles have 3+ different colors. The optimal split algorithm must be fast (runs per minimap cell per frame).

**Fast heuristic:** Use the most common terrain color as bg, second most common as fg. Assign each quadrant to whichever is closer.

**Rating: 8/10** -- Excellent density. The 2-color quantization adds complexity but the result is visually impressive.

### 9.4 Sextant Minimap (6x Density)

Use sextant characters for 2x3 sub-pixels per cell. Same algorithm as quadrant but with 6-bit patterns and 64 characters.

A 30x20 minimap shows 60x60 terrain tiles.

**Rating: 6/10** -- Great density but font support issues. Implement as a premium option, not default.

### 9.5 Minimap Entity Markers

Overlay bright single-character markers on the minimap for important features:

| Feature       | Marker | Color            |
|--------------|--------|------------------|
| Player/camera | `+`    | White            |
| Settlement    | `■`    | Warm yellow      |
| Enemy         | `!`    | Red              |
| Resource      | `◆`    | Color by type    |
| Water source  | `○`    | Blue             |

Markers should always render on top of terrain, at full brightness (ignore day/night tinting).

**Rating: 7/10** -- Essential for gameplay. A minimap without markers is just a pretty picture.

---

## 10. Notable Projects and Tools

### 10.1 MapSCII -- Terminal World Map

**Repo:** [rastapasta/mapscii](https://github.com/rastapasta/mapscii)
**Try it:** `telnet mapscii.me`

**Techniques used:**
- Braille characters (default) or block characters (toggle with `c`)
- Vector tile rendering from OpenStreetMap data
- Bresenham line algorithm for roads/boundaries
- Earcut polygon triangulation for filled areas (lakes, forests, buildings)
- rbush R-tree spatial indexing for efficient feature lookup
- xterm-256 color palette (NOT 24-bit true color)
- Dynamic zoom with tile re-rendering at each level

**Relevant lesson for us:** MapSCII proves that braille rendering produces excellent results for LINE features (roads, rivers, coastlines) but struggles with FILLED areas at character resolution. For terrain, we want the opposite -- filled areas are primary. This confirms that half-block/quadrant is better for our terrain minimap, while braille is better for overlays (contours, flow vectors).

### 10.2 Chafa -- Image to Terminal Converter

**Repo:** [hpjansson/chafa](https://hpjansson.org/chafa/)

**Techniques used:**
- Multiple symbol modes: vhalf (half-blocks only), block, sextant, braille, wedge, all
- Dithering: none, ordered (Bayer), diffusion (Floyd-Steinberg)
- Grain control: 1x1, 2x1, 2x2, 4x4, 8x8 -- determines sub-cell resolution
- Color space: Works in 256-color or 24-bit true color
- Automatic terminal capability detection
- Fallback chain: sixel -> kitty -> iTerm2 -> symbols

**Key insight:** Chafa's "grain" concept is useful. Each cell is subdivided into a grain grid, and the rendering algorithm picks the character that best approximates that grain pattern. For terrain rendering, we could use a similar approach: pre-render each terrain type as a small "texture tile" and use Chafa-style matching to select characters.

**Relevant lesson:** Chafa's performance data shows that character-mode rendering is 10-50x faster than pixel protocols (sixel/kitty) for the same visual area. For our game that needs 30+ FPS, character-mode with half-blocks is the pragmatic choice.

### 10.3 Notcurses -- Terminal Graphics Library

**Repo:** [dankamongmen/notcurses](https://github.com/dankamongmen/notcurses)

**Blitter hierarchy (best to worst quality):**
1. NCBLIT_PIXEL (sixel/kitty) -- actual pixels
2. NCBLIT_3x2 (sextants) -- 6 sub-pixels/cell
3. NCBLIT_2x2 (quadrants) -- 4 sub-pixels/cell  
4. NCBLIT_2x1 (half-blocks) -- 2 sub-pixels/cell [DEFAULT]
5. NCBLIT_1x1 (spaces) -- 1 pixel/cell

**Performance benchmarks (from notcurses-demo):**
- libvte terminals (GNOME Terminal, Tilix): ~75 FPS
- kitty: ~73 FPS
- alacritty: ~70 FPS
- foot: ~70 FPS
- xterm: ~12 FPS (avoid)

**Relevant lesson:** We don't need Notcurses as a dependency, but its blitter hierarchy informs our implementation priorities. Start with half-blocks (universal, fast), offer quadrant/sextant as user options.

### 10.4 Drawille -- Braille Canvas Library

**Repo:** [asciimoo/drawille](https://github.com/asciimoo/drawille)

Python library for drawing on a 2D braille canvas. API:
```python
canvas = Canvas()
canvas.set(x, y)          # Turn on a braille dot
canvas.unset(x, y)        # Turn off a braille dot  
canvas.toggle(x, y)       # Toggle a braille dot
canvas.frame()             # Render canvas to string
```

**Relevant lesson:** The API is clean and worth mimicking for our contour overlay system. A `BrailleCanvas` struct in Rust that renders to our existing buffer would enable contour lines, flow vectors, and wind patterns.

### 10.5 img2unicode -- Optimal Character Selection

**Repo:** [matrach/img2unicode](https://github.com/matrach/img2unicode)

**The algorithm:** For each image patch (one terminal cell), img2unicode:
1. Pre-renders all candidate characters as binary masks at the cell's pixel resolution
2. For each mask, computes the optimal fg and bg colors as weighted averages
3. Computes squared error between (mask * fg + (1-mask) * bg) and the actual image
4. Selects the character with minimum total error

This is exhaustive search over 5,553+ character templates with analytic color solution. O(S * T) where T = template count.

**Relevant lesson for us:** We don't need this full optimization for terrain (we hand-pick characters). But for the minimap, using a simplified version of this algorithm to select the best quadrant/sextant character for each cell cluster would produce optimal results. The key insight: compute optimal fg/bg colors analytically, don't search over colors.

### 10.6 blessed-contrib -- Terminal Dashboards

**Repo:** [yaronn/blessed-contrib](https://github.com/yaronn/blessed-contrib)

JavaScript terminal dashboard library with sparklines, line charts, bar charts, world map, and gauge widgets.

**Relevant lesson:** The sparkline widget uses block elements (eighth-blocks: `▁▂▃▄▅▆▇█`) for vertical bar charts. We could use the same characters for a terrain elevation profile view -- a horizontal slice through the terrain showing the height profile as a sparkline.

### 10.7 ascii-fluid -- Terminal Fluid Simulation

**Repo:** [esimov/ascii-fluid](https://github.com/esimov/ascii-fluid)

ASCII fluid dynamics in the terminal, controlled by webcam input. Uses character density to represent fluid density.

**Relevant lesson:** Fluid density mapped to character density (`.`, `:`, `%`, `#`, `@`) creates a convincing fluid visualization. We could apply this to our pipe_water system: show water flow intensity through character density rather than just color.

---

## 11. Demoscene and Terminal Art Techniques

### 11.1 ANSI Art as Procedural Texture

Demoscene productions create complex imagery from character-level building blocks. Key techniques applicable to terrain:

**Gradient fills using shade characters:** `░▒▓█` create smooth 4-step gradients. With fg/bg color modulation, this becomes a near-continuous gradient:

```
// 8-level gradient using 4 shade chars x 2 color roles
Level 1: ░ with dark fg on black bg
Level 2: ░ with medium fg on dark bg  
Level 3: ▒ with medium fg on dark bg
Level 4: ▒ with bright fg on medium bg
Level 5: ▓ with bright fg on medium bg
Level 6: ▓ with white fg on bright bg
Level 7: █ with white fg on bright bg
Level 8: █ with white fg on white bg (solid)
```

This gives 8 visually distinct density levels per color pair -- useful for fog of war gradients, smoke, mist effects.

### 11.2 Frame-Rate-Independent Animation

Stone Story RPG's technique: define animation as a sequence of "keyframes" (specific characters/colors) with interpolation between them. For terminal rendering:

```rust
struct AnimatedTerrain {
    frames: Vec<(char, Color)>,
    period_ticks: u32,
    offset_hash: bool, // If true, offset phase by position hash
}

fn current_frame(&self, tick: u64, wx: usize, wy: usize) -> (char, Color) {
    let phase = if self.offset_hash {
        (tick + fast_hash(wx, wy) as u64) % self.period_ticks as u64
    } else {
        tick % self.period_ticks as u64
    };
    let t = phase as f64 / self.period_ticks as f64;
    let idx = (t * self.frames.len() as f64) as usize;
    self.frames[idx.min(self.frames.len() - 1)]
}
```

The `offset_hash` is critical: without it, all water tiles animate in sync (looks artificial). With it, each tile has a different phase offset, creating natural-looking ripple patterns.

### 11.3 CRT Post-Processing Effect

Caves of Qud-style CRT simulation:

```rust
fn apply_crt_effect(frame: &mut FrameBuffer) {
    for y in 0..frame.height {
        // Scanline darkening: every other row is slightly darker
        let scanline = if y % 2 == 0 { 1.0 } else { 0.85 };
        
        for x in 0..frame.width {
            // Vignette: darken edges
            let cx = x as f64 / frame.width as f64 - 0.5;
            let cy = y as f64 / frame.height as f64 - 0.5;
            let vignette = 1.0 - (cx * cx + cy * cy) * 0.8;
            
            let factor = scanline * vignette;
            frame.fg[y][x] = scale_color(frame.fg[y][x], factor);
            frame.bg[y][x] = scale_color(frame.bg[y][x], factor);
        }
    }
}
```

**Rating: 4/10** -- Fun novelty but reduces readability. Offer as a toggle for players who want the retro aesthetic.

---

## 12. Fog, Depth Cues, and Atmospheric Perspective

### 12.1 Atmospheric Perspective (Distance Desaturation)

In real landscapes, distant features appear bluer, lighter, and less saturated due to atmospheric scattering. Implement in OKLab:

```rust
fn atmospheric_perspective(color: Color, distance: f64, max_distance: f64) -> Color {
    let t = (distance / max_distance).clamp(0.0, 1.0);
    let (l, a, b) = srgb_to_oklab(color.0, color.1, color.2);
    
    // Atmosphere color in OKLab (light blue-gray)
    let atmo = srgb_to_oklab(180, 190, 210);
    
    // Desaturate and shift toward atmosphere
    let new_l = lerp(l, atmo.0, t * 0.4);
    let new_a = lerp(a, atmo.1, t * 0.6);  // Desaturate faster than lighten
    let new_b = lerp(b, atmo.2, t * 0.6);
    
    let (r, g, b) = oklab_to_srgb(new_l, new_a, new_b);
    Color(r, g, b)
}
```

**Applied to:** Terrain cells far from the camera center (edges of viewport). Creates a natural depth-of-field effect where the map center is vivid and edges fade toward hazy blue.

**Rating: 8/10** -- Cheap, effective, and gives the viewport a sense of physical depth that most terminal games lack entirely.

### 12.2 Elevation-Based Atmospheric Thinning

Mountains above a certain elevation should appear clearer (thinner atmosphere), while valleys should appear hazier. This is the opposite of distance fog:

```rust
fn elevation_clarity(base_haze: f64, elevation: f64) -> f64 {
    // Higher = clearer
    let altitude_factor = (elevation / 1000.0).clamp(0.0, 1.0);
    base_haze * (1.0 - altitude_factor * 0.5)
}
```

Valleys shrouded in mist while mountaintops gleam sharp and clear -- this is how real landscapes look.

**Rating: 7/10** -- Adds realistic depth, pairs beautifully with height fog (Section 4.3).

### 12.3 Depth-Based Character Simplification

As terrain cells get farther from camera, simplify their characters:

| Distance  | Detail level | Example (forest)    |
|----------|-------------|---------------------|
| 0-20 cells  | Full detail  | `♣` with full color  |
| 20-40 cells | Reduced      | `#` with muted color |
| 40+ cells   | Minimal      | `·` or ` ` (just bg) |

This mimics level-of-detail in 3D rendering. Distant terrain becomes just colored blocks, while nearby terrain shows rich character textures. Performance benefit: fewer distinct characters = better terminal buffer compression.

**Rating: 6/10** -- Interesting concept but our viewport isn't large enough to benefit much from LOD. More useful if we add a zoom-out mode.

---

## 13. Implementation Priority Matrix

Ranked by (visual impact * feasibility) / implementation cost:

### Tier 1: Implement Immediately (< 1 day each, massive impact)

| Technique | Section | Impact | Cost | Notes |
|-----------|---------|--------|------|-------|
| Cloud shadows | 4.5 | 9/10 | Trivial | Perlin noise + scroll |
| Rain overlay | 4.1 | 9/10 | Trivial | Hash-based post-process |
| OKLab color interpolation | 6.1 | 9/10 | Small | 40 lines of math |
| Half-block minimap | 9.2 | 9/10 | Trivial | 10 lines |
| Shore foam animation | 3.4 | 8/10 | Trivial | Border detection + char cycle |
| Ambient occlusion | 5.3 | 8/10 | Small | Precomputed, cached |
| Anti-banding noise | 6.3 | 7/10 | Trivial | Hash + lightness offset |

### Tier 2: Implement Soon (1-3 days each, high impact)

| Technique | Section | Impact | Cost | Notes |
|-----------|---------|--------|------|-------|
| Point light system | 5.6 | 9/10 | Medium | Vec<PointLight> + additive blend |
| Snow accumulation | 4.2 | 8/10 | Medium | Per-tile depth + color shift |
| Flow-directional water | 3.1 | 8/10 | Medium | Needs flow data access |
| Ordered dither biome transitions | 7.1 | 8/10 | Small | Bayer matrix lookup |
| Atmospheric perspective | 12.1 | 8/10 | Small | OKLab distance fade |
| Height fog in valleys | 4.3 | 8/10 | Small | Elevation threshold + blend |
| Braille contour overlay | 2.1 | 9/10 | Medium | New overlay mode |
| Quadrant minimap | 9.3 | 8/10 | Medium | Color clustering algorithm |

### Tier 3: Implement When Time Allows (3+ days, nice-to-have)

| Technique | Section | Impact | Cost | Notes |
|-----------|---------|--------|------|-------|
| Per-side wall lighting | 5.1 | 7/10 | High | Architecture change |
| God rays | 5.5 | 6/10 | High | Per-cell ray march |
| Water caustics | 3.3 | 7/10 | Medium | Voronoi noise per water cell |
| Sextant minimap | 9.4 | 6/10 | Medium | Font support concerns |
| Diagonal slope chars | 1.5 | 5/10 | Low | Font support concerns |
| CRT post-process | 11.3 | 4/10 | Low | Novelty feature |
| Temporal dithering | 7.2 | 5/10 | Low | Only if banding visible |
| Lightning flash | 4.4 | 7/10 | Low | Occasional dramatic effect |

---

## Appendix A: Unicode Character Reference for Terrain

### Block Elements (U+2580-U+259F)
```
▀ U+2580  Upper half block
▁ U+2581  Lower one eighth block
▂ U+2582  Lower one quarter block
▃ U+2583  Lower three eighths block
▄ U+2584  Lower half block
▅ U+2585  Lower five eighths block
▆ U+2586  Lower three quarters block
▇ U+2587  Lower seven eighths block
█ U+2588  Full block
▉ U+2589  Left seven eighths block
▊ U+258A  Left three quarters block
▋ U+258B  Left five eighths block
▌ U+258C  Left half block
▍ U+258D  Left three eighths block
▎ U+258E  Left one quarter block
▏ U+258F  Left one eighth block
▐ U+2590  Right half block
░ U+2591  Light shade
▒ U+2592  Medium shade
▓ U+2593  Dark shade
▔ U+2594  Upper one eighth block
▕ U+2595  Right one eighth block
▖ U+2596  Quadrant lower left
▗ U+2597  Quadrant lower right
▘ U+2598  Quadrant upper left
▙ U+2599  Quadrant upper left and lower left and lower right
▚ U+259A  Quadrant upper left and lower right
▛ U+259B  Quadrant upper left and upper right and lower left
▜ U+259C  Quadrant upper left and upper right and lower right
▝ U+259D  Quadrant upper right
▞ U+259E  Quadrant upper right and lower left
▟ U+259F  Quadrant upper left and upper right and lower left and lower right
```

### Shade Characters for Terrain Density
```
░ ~25% fill  -- Sparse vegetation, explored fog of war
▒ ~50% fill  -- Medium density, partial cover
▓ ~75% fill  -- Dense canopy, heavy fog
█ ~100% fill -- Solid wall, deep water
```

### Box Drawing for Roads and Buildings (U+2500-U+257F)
```
Light:   ─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼
Heavy:   ━ ┃ ┏ ┓ ┗ ┛ ┣ ┫ ┳ ┻ ╋  
Double:  ═ ║ ╔ ╗ ╚ ╝ ╠ ╣ ╦ ╩ ╬
Rounded: ╭ ╮ ╰ ╯
```

### Braille Pattern Encoding (U+2800-U+28FF)
```
Bit layout:       Encoding:
 [0] [3]          char = '\u{2800}' + bitfield
 [1] [4]          bit0=top-left, bit1=mid-left, bit2=bot-left
 [2] [5]          bit3=top-right, bit4=mid-right, bit5=bot-right
 [6] [7]          bit6=bottom-left, bit7=bottom-right
```

### Eighth Blocks for Sparkline Elevation Profiles
```
▁ U+2581  1/8
▂ U+2582  2/8
▃ U+2583  3/8
▄ U+2584  4/8
▅ U+2585  5/8
▆ U+2586  6/8
▇ U+2587  7/8
█ U+2588  8/8
```

### Geometric Shapes for Map Markers (U+25A0-U+25FF)
```
■ U+25A0  Black square (settlement)
□ U+25A1  White square (ruin)
▲ U+25B2  Black triangle (mountain peak)
△ U+25B3  White triangle (waypoint)
● U+25CF  Black circle (resource)
○ U+25CB  White circle (water source)
◆ U+25C6  Black diamond (special feature)
◇ U+25C7  White diamond (discovered feature)
★ U+2605  Black star (capital/important)
```

---

## Appendix B: Key Sources

### Projects and Repos
- [MapSCII -- Terminal world map](https://github.com/rastapasta/mapscii)
- [Chafa -- Terminal graphics converter](https://hpjansson.org/chafa/)
- [Notcurses -- Terminal graphics library](https://github.com/dankamongmen/notcurses)
- [Drawille -- Braille canvas](https://github.com/asciimoo/drawille)
- [img2unicode -- Optimal character selection](https://github.com/matrach/img2unicode)
- [blessed-contrib -- Terminal dashboards](https://github.com/yaronn/blessed-contrib)
- [pixterm -- Terminal image display](https://github.com/eliukblau/pixterm)
- [ascii-fluid -- Terminal fluid simulation](https://github.com/esimov/ascii-fluid)
- [WeatherSpect -- ASCII weather simulation](https://github.com/AnotherFoxGuy/weatherspect)
- [Terminal Rain-Lightning](https://github.com/rmaake1/terminal-rain-lightning)
- [BrogueCE -- Roguelike with advanced lighting](https://github.com/tmewett/BrogueCE)
- [Gridbugs -- Roguelike lighting demos](https://www.gridbugs.org/roguelike-lighting-demo/)
- [ratatui-image -- Image rendering for ratatui](https://lib.rs/crates/ratatui-image)

### Color Science
- [OKLab -- Bjorn Ottosson](https://bottosson.github.io/posts/oklab/)
- [OKLCH in CSS -- Evil Martians](https://evilmartians.com/chronicles/oklch-in-css-why-quit-rgb-hsl)
- [Ditherpunk -- Monochrome dithering article](https://surma.dev/things/ditherpunk/)
- [Color banding and gradients -- Frost.kiwi](https://blog.frost.kiwi/GLSL-noise-and-radial-gradient/)

### Game Art References
- [Cogmind ASCII Art -- Grid Sage Games](https://www.gridsagegames.com/blog/2014/03/cogmind-ascii-art-making/)
- [Stone Story RPG ASCII Tutorial](https://stonestoryrpg.com/ascii_tutorial.html)
- [Caves of Qud Visual Style](https://wiki.cavesofqud.com/wiki/Visual_Style)

### Unicode References
- [Block Elements (U+2580-U+259F)](https://www.unicode.org/charts/PDF/U2580.pdf)
- [Symbols for Legacy Computing (U+1FB00-U+1FBFF)](https://www.unicode.org/charts/PDF/U1FB00.pdf)
- [Geometric Shapes (U+25A0-U+25FF)](https://www.unicode.org/charts/PDF/U25A0.pdf)
- [Unicode graphics overview -- Dernocua](https://dernocua.github.io/notes/unicode-graphics.html)
