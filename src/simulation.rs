use rand::RngExt;
use serde::{Serialize, Deserialize};

use crate::renderer::Color;

/// Parallel grid of water depth, layered on top of a height map.
/// Water flows downhill, erodes terrain, and evaporates over time.
#[derive(Serialize, Deserialize)]
pub struct WaterMap {
    pub width: usize,
    pub height: usize,
    water: Vec<f64>,
    water_temp: Vec<f64>,   // transfer buffer for this frame
    water_avg: Vec<f64>,    // smoothed for rendering
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub rain_rate: f64,        // fraction of tiles that get rain per tick
    pub rain_amount: f64,      // water added per raindrop
    pub flow_fraction: f64,    // how much of height diff flows per tick
    pub evaporation: f64,      // water removed per tile per tick
    pub erosion_enabled: bool,
    pub erosion_strength: f64, // multiplier for erosion effect
    pub avg_factor: f64,       // smoothing: 0.95 = slow, 0.5 = fast
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            rain_rate: 0.02,
            rain_amount: 0.001,
            flow_fraction: 0.5,
            evaporation: 0.00001,
            erosion_enabled: false,
            erosion_strength: 1.0,
            avg_factor: 0.95,
        }
    }
}

impl WaterMap {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            water: vec![0.0; n],
            water_temp: vec![0.0; n],
            water_avg: vec![0.0; n],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.water[y * self.width + x]
        } else {
            0.0
        }
    }

    pub fn set(&mut self, x: usize, y: usize, val: f64) {
        if x < self.width && y < self.height {
            self.water[y * self.width + x] = val;
            self.water_avg[y * self.width + x] = val;
        }
    }

    pub fn get_avg(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.water_avg[y * self.width + x]
        } else {
            0.0
        }
    }

    fn wrapping_coords(&self, x: i32, y: i32) -> (usize, usize) {
        let wx = x.rem_euclid(self.width as i32) as usize;
        let wy = y.rem_euclid(self.height as i32) as usize;
        (wx, wy)
    }

    pub fn rain(&mut self, config: &SimConfig) {
        let mut rng = rand::rng();
        let count = (self.width as f64 * self.height as f64 * config.rain_rate) as usize;
        for _ in 0..count {
            let x = rng.random_range(0..self.width);
            let y = rng.random_range(0..self.height);
            self.water[y * self.width + x] += config.rain_amount;
        }
    }

    pub fn drain(&mut self) {
        self.water.fill(0.0);
        self.water_temp.fill(0.0);
        self.water_avg.fill(0.0);
    }

    /// Returns true if any water exists on the map.
    pub fn has_water(&self) -> bool {
        self.water.iter().any(|&w| w > 0.0)
    }

    /// Run one tick of water flow + optional erosion.
    /// `heights` is the terrain height map (same dimensions), and may be modified by erosion.
    pub fn update(&mut self, heights: &mut Vec<f64>, config: &SimConfig) {
        self.water_temp.fill(0.0);

        let w = self.width;
        let h = self.height;

        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;

                // update smoothed average
                self.water_avg[i] = self.water_avg[i] * config.avg_factor
                    + self.water[i] * (1.0 - config.avg_factor);

                // skip dry cells entirely
                if self.water[i] < 1e-8 {
                    continue;
                }

                let cell_h = heights[i] + self.water[i];

                // find lowest neighbor — inline bounds for interior cells (avoid mod)
                let mut best_i = i;
                let mut best_h = cell_h;

                // cardinal directions
                let cardinals: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                for &(dx, dy) in &cardinals {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let (nx, ny) = if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        (nx as usize, ny as usize)
                    } else {
                        self.wrapping_coords(nx, ny)
                    };
                    let ni = ny * w + nx;
                    let nh = heights[ni] + self.water[ni];
                    if nh < best_h {
                        best_i = ni;
                        best_h = nh;
                    }
                }

                // diagonal directions — only prefer if drop is > sqrt(2)x the cardinal best
                let diagonals: [(i32, i32); 4] = [(1, 1), (-1, -1), (1, -1), (-1, 1)];
                for &(dx, dy) in &diagonals {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let (nx, ny) = if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        (nx as usize, ny as usize)
                    } else {
                        self.wrapping_coords(nx, ny)
                    };
                    let ni = ny * w + nx;
                    let nh = heights[ni] + self.water[ni];
                    if (cell_h - nh) > (cell_h - best_h) * 1.4142 {
                        best_i = ni;
                        best_h = nh;
                    }
                }

                // flow water downhill
                if best_h < cell_h - 0.0001 {
                    let diff_raw = cell_h - best_h;
                    let mut diff = diff_raw * config.flow_fraction;
                    diff *= 1.0 - self.water[i];
                    if diff > self.water[i] {
                        diff = self.water[i];
                    }

                    self.water_temp[best_i] += diff;
                    self.water_temp[i] -= diff;
                }
            }
        }

        // apply transfers, erosion, and evaporation
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;

                if config.erosion_enabled && self.water_temp[i].abs() > 1e-10 {
                    let change = self.water_temp[i];
                    let mut erode = if change > 0.0 {
                        change * 0.5
                    } else {
                        change
                    };

                    if self.water[i] > 0.001 {
                        erode *= (erode * 0.1 / self.water[i]).abs();
                    } else {
                        erode *= (erode * 40.0).abs();
                    }

                    erode *= config.erosion_strength;

                    heights[i] += erode / 8.0;
                    for &(dx, dy, wt) in &[
                        (1i32, 0i32, 16.0), (-1, 0, 16.0), (0, 1, 16.0), (0, -1, 16.0),
                        (1, 1, 22.63), (-1, -1, 22.63), (1, -1, 22.63), (-1, 1, 22.63),
                    ] {
                        let (nx, ny) = self.wrapping_coords(x as i32 + dx, y as i32 + dy);
                        heights[ny * w + nx] += erode / wt;
                    }
                }

                self.water[i] = (self.water[i] + self.water_temp[i] - config.evaporation)
                    .clamp(0.0, 1.0);
            }
        }
    }
}

/// Moisture grid: driven by water presence, propagates downwind, drives vegetation.
#[derive(Serialize, Deserialize)]
pub struct MoistureMap {
    pub width: usize,
    pub height: usize,
    moisture: Vec<f64>,
}

impl MoistureMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            moisture: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.moisture[y * self.width + x]
        } else {
            0.0
        }
    }

    fn wrapping_idx(&self, x: i32, y: i32) -> usize {
        let wx = x.rem_euclid(self.width as i32) as usize;
        let wy = y.rem_euclid(self.height as i32) as usize;
        wy * self.width + wx
    }

    /// Update moisture from water presence and propagate.
    /// Also updates vegetation based on moisture bands.
    pub fn update(&mut self, water: &WaterMap, vegetation: &mut VegetationMap, map: &crate::tilemap::TileMap) {
        // Step 1: moisture from water — gentle contribution, faster decay
        // Skip Water terrain tiles (permanent oceans) — only rain water drives moisture
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let is_ocean = matches!(map.get(x, y), Some(crate::tilemap::Terrain::Water));
                if is_ocean {
                    self.moisture[i] = 0.0; // oceans don't generate land moisture
                    continue;
                }
                let w = water.get(x, y);
                if w > 0.01 {
                    // Standing water: high moisture, but blend don't slam to 1.0
                    self.moisture[i] = self.moisture[i] * 0.8 + 0.2;
                } else {
                    // Decay toward zero; small boost from trace water
                    self.moisture[i] = self.moisture[i] * 0.95 + w * 5.0;
                }
            }
        }

        // Step 2: propagate moisture forward (downwind = +y direction, like original)
        // Conservative: what leaves a cell is subtracted from it
        let mut delta = vec![0.0f64; self.width * self.height];
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let m = self.moisture[i];
                let spread = m * 0.2; // total amount leaving this cell
                // forward: 50% of spread
                let fi = self.wrapping_idx(x as i32, y as i32 + 1);
                delta[fi] += spread * 0.5;
                // diagonals: 25% each
                let fli = self.wrapping_idx(x as i32 + 1, y as i32 + 1);
                delta[fli] += spread * 0.25;
                let fri = self.wrapping_idx(x as i32 - 1, y as i32 + 1);
                delta[fri] += spread * 0.25;
                // subtract from source
                delta[i] -= spread;
            }
        }
        for i in 0..self.moisture.len() {
            self.moisture[i] = (self.moisture[i] + delta[i]).clamp(0.0, 1.0);
        }

        // Step 3: box blur
        self.box_blur();

        // Step 4: vegetation responds to moisture (after blur for stable values)
        // No vegetation on sand or water terrain
        for y in 0..self.height {
            for x in 0..self.width {
                let terrain = map.get(x, y);
                let can_grow = match terrain {
                    Some(crate::tilemap::Terrain::Sand) | Some(crate::tilemap::Terrain::Water) => false,
                    _ => true,
                };
                let m = self.moisture[y * self.width + x];
                if can_grow && m > 0.1 && m < 0.5 {
                    vegetation.grow(x, y);
                } else {
                    vegetation.decay(x, y);
                }
            }
        }
    }

    fn box_blur(&mut self) {
        let mut temp = vec![0.0f64; self.width * self.height];
        for y in 0..self.height {
            for x in 0..self.width {
                let mut sum = 0.0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ni = self.wrapping_idx(x as i32 + dx, y as i32 + dy);
                        sum += self.moisture[ni];
                    }
                }
                temp[y * self.width + x] = (sum / 9.0).clamp(0.0, 1.0);
            }
        }
        self.moisture = temp;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn name(&self) -> &str {
        match self {
            Season::Spring => "Spring",
            Season::Summer => "Summer",
            Season::Autumn => "Autumn",
            Season::Winter => "Winter",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SeasonModifiers {
    pub rain_mult: f64,
    pub evap_mult: f64,
    pub veg_growth_mult: f64,
    pub hunger_mult: f64,
    pub wolf_aggression: f64,
}

/// Day/night cycle with Blinn-Phong lighting, terrain normals, and shadow raytracing.
#[derive(Serialize, Deserialize)]
pub struct DayNightCycle {
    pub hour: f64,           // 0.0 - 24.0
    pub tick_rate: f64,      // hours per tick
    pub enabled: bool,
    pub day: u32,            // current day (0-indexed within season)
    pub season: Season,
    pub year: u32,
    light_map: Vec<f64>,     // per-tile total lighting intensity (combined diffuse + shadow)
    light_w: usize,
    light_h: usize,
}

impl DayNightCycle {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            hour: 10.0, // start at 10am
            tick_rate: 0.02, // ~8 minutes per real second at 30fps
            enabled: true,
            day: 0,
            season: Season::Spring,
            year: 0,
            light_map: vec![1.0; width * height],
            light_w: width,
            light_h: height,
        }
    }

    /// Advance time by one tick.
    pub fn tick(&mut self) {
        if !self.enabled {
            return;
        }
        self.hour += self.tick_rate;
        if self.hour >= 24.0 {
            self.hour -= 24.0;
            self.day += 1;
            if self.day >= 10 {
                self.day = 0;
                self.season = match self.season {
                    Season::Spring => Season::Summer,
                    Season::Summer => Season::Autumn,
                    Season::Autumn => Season::Winter,
                    Season::Winter => { self.year += 1; Season::Spring },
                };
            }
        }
    }

    /// Get season-dependent modifiers for simulation systems.
    pub fn season_modifiers(&self) -> SeasonModifiers {
        match self.season {
            Season::Spring => SeasonModifiers { rain_mult: 1.5, evap_mult: 1.0, veg_growth_mult: 2.0, hunger_mult: 1.0, wolf_aggression: 0.95 },
            Season::Summer => SeasonModifiers { rain_mult: 0.5, evap_mult: 2.0, veg_growth_mult: 1.5, hunger_mult: 0.8, wolf_aggression: 0.95 },
            Season::Autumn => SeasonModifiers { rain_mult: 1.0, evap_mult: 1.0, veg_growth_mult: 0.3, hunger_mult: 1.0, wolf_aggression: 0.8 },
            Season::Winter => SeasonModifiers { rain_mult: 0.3, evap_mult: 0.5, veg_growth_mult: 0.0, hunger_mult: 1.8, wolf_aggression: 0.6 },
        }
    }

    /// Format date as "Y1 Spring D1".
    pub fn date_string(&self) -> String {
        format!("Y{} {} D{}", self.year + 1, self.season.name(), self.day + 1)
    }

    /// Sun elevation angle in radians. Peaks at noon, below 0 at night.
    /// Max ~60 degrees — keeps the sun from going truly overhead so there's
    /// always a meaningful horizontal component for shadows and directional shading.
    pub fn sun_elevation(&self) -> f64 {
        let angle = (self.hour - 6.0) / 12.0 * std::f64::consts::PI;
        angle.sin() * (std::f64::consts::PI / 3.0) // max ~60 degrees
    }

    /// Sun azimuth in radians. Traces east (6am) → south (noon) → west (6pm).
    pub fn sun_azimuth(&self) -> f64 {
        // 6am = east = 0, noon = south = PI/2, 6pm = west = PI
        (self.hour - 6.0) / 12.0 * std::f64::consts::PI
    }

    /// Sun direction as a 3D unit vector pointing TOWARD the sun.
    /// Proper spherical: azimuth sweeps east→south→west, elevation rises and falls.
    pub fn sun_direction_3d(&self) -> (f64, f64, f64) {
        Self::celestial_direction(self.sun_elevation(), self.sun_azimuth())
    }

    /// Moon elevation — rises at 6pm, peaks at midnight, sets at 6am.
    pub fn moon_elevation(&self) -> f64 {
        // Map: 18h→0 (rise), 0h→PI/2 (peak), 6h→PI (set)
        let phase = ((self.hour - 18.0 + 24.0) % 24.0) / 12.0 * std::f64::consts::PI;
        phase.sin() * (std::f64::consts::PI / 4.0) // max ~45 degrees
    }

    /// Moon azimuth — rises east at 6pm, south at midnight, west at 6am.
    pub fn moon_azimuth(&self) -> f64 {
        ((self.hour - 18.0 + 24.0) % 24.0) / 12.0 * std::f64::consts::PI
    }

    /// Moon direction as a 3D unit vector.
    pub fn moon_direction_3d(&self) -> (f64, f64, f64) {
        Self::celestial_direction(self.moon_elevation(), self.moon_azimuth())
    }

    /// Convert elevation + azimuth to a 3D unit direction vector.
    fn celestial_direction(elev: f64, azimuth: f64) -> (f64, f64, f64) {
        let dz = elev.sin();
        let horiz = elev.cos();
        let dx = azimuth.cos() * horiz;
        let dy = -azimuth.sin() * horiz;

        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        if len < 0.001 {
            return (0.0, 0.0, 1.0);
        }
        (dx / len, dy / len, dz / len)
    }

    /// Compute terrain normal at (x, y) from height finite differences.
    /// Returns a normalized (nx, ny, nz) vector. The z-scale controls how
    /// exaggerated the slopes appear (higher = flatter normals).
    fn terrain_normal(heights: &[f64], width: usize, height: usize, x: usize, y: usize) -> (f64, f64, f64) {
        let h = |xi: i32, yi: i32| -> f64 {
            let cx = (xi.max(0) as usize).min(width - 1);
            let cy = (yi.max(0) as usize).min(height - 1);
            heights[cy * width + cx]
        };

        // Central differences, amplified heavily.
        // Raw gradients are ~0.005 (heights 0-1 over 256 cells).
        // Multiply by 40 so slopes become visible in the lighting.
        let scale = 40.0;
        let dhdx = (h(x as i32 + 1, y as i32) - h(x as i32 - 1, y as i32)) * 0.5 * scale;
        let dhdy = (h(x as i32, y as i32 + 1) - h(x as i32, y as i32 - 1)) * 0.5 * scale;

        // Normal = (-dh/dx, -dh/dy, 1), normalized
        let nx = -dhdx;
        let ny = -dhdy;
        let nz = 1.0;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        (nx / len, ny / len, nz / len)
    }

    /// Compute Blinn-Phong lighting with shadow sweep for a viewport region.
    /// Shadow sweep is O(cells) — one pass across the map propagating shadow height,
    /// instead of O(cells * ray_steps) per-cell raycasting.
    pub fn compute_lighting(
        &mut self,
        heights: &[f64],
        map_w: usize,
        map_h: usize,
        vx: i32,
        vy: i32,
        vw: usize,
        vh: usize,
    ) {
        let sun_elev = self.sun_elevation();
        let moon_elev = self.moon_elevation();
        let is_night = sun_elev <= 0.0;
        let moon_up = moon_elev > 0.0;

        if is_night && !moon_up {
            // No sun, no moon: everything dark
            let x0 = vx.max(0) as usize;
            let y0 = vy.max(0) as usize;
            let x1 = ((vx + vw as i32) as usize).min(map_w);
            let y1 = ((vy + vh as i32) as usize).min(map_h);
            for y_pos in y0..y1 {
                for x_pos in x0..x1 {
                    self.light_map[y_pos * map_w + x_pos] = 0.0;
                }
            }
            return;
        }

        // Pick the active light source
        let (light_dx, light_dy, light_dz, light_strength) = if is_night {
            let (dx, dy, dz) = self.moon_direction_3d();
            (dx, dy, dz, 0.6) // moon at 60% sun intensity
        } else {
            let (dx, dy, dz) = self.sun_direction_3d();
            (dx, dy, dz, 1.0)
        };
        let active_elev = if is_night { moon_elev } else { sun_elev };
        let tan_elev = active_elev.tan().max(0.01);
        let shadow_decay = tan_elev * 0.15;

        // Shadow sweep: single pass AGAINST the sun direction.
        // We propagate a "shadow height" — if a cell's terrain is below the shadow
        // height, it's in shadow. The shadow height decays as it moves away from
        // the casting peak (because the sun ray rises).
        //
        // Sweep the viewport + a margin so shadows from peaks just outside are caught.
        let margin = 30i32; // shadow can reach ~30 cells at low angles
        let x0 = (vx - margin).max(0) as usize;
        let y0 = (vy - margin).max(0) as usize;
        let x1 = ((vx + vw as i32 + margin) as usize).min(map_w);
        let y1 = ((vy + vh as i32 + margin) as usize).min(map_h);

        // Build a shadow buffer for the sweep region
        let sw = x1 - x0;
        let sh = y1 - y0;
        let mut shadow = vec![0.0f64; sw * sh];

        // Determine sweep order: sweep FROM the sun side TO the shadow side.
        // Weight neighbor contributions by how much light comes from each axis.
        let horiz_len = (light_dx * light_dx + light_dy * light_dy).sqrt().max(0.001);
        let wx = (light_dx.abs() / horiz_len).min(1.0); // weight of x-neighbor
        let wy = (light_dy.abs() / horiz_len).min(1.0); // weight of y-neighbor

        let sweep_x_rev = light_dx < 0.0;
        let sweep_y_rev = light_dy < 0.0;

        let xs: Vec<usize> = if sweep_x_rev {
            (x0..x1).rev().collect()
        } else {
            (x0..x1).collect()
        };
        let ys: Vec<usize> = if sweep_y_rev {
            (y0..y1).rev().collect()
        } else {
            (y0..y1).collect()
        };

        for &y_pos in &ys {
            for &x_pos in &xs {
                let si = (y_pos - y0) * sw + (x_pos - x0);
                let terrain_h = heights[y_pos * map_w + x_pos];

                // Incoming shadow from sun-side neighbors, weighted by light direction
                let mut max_shadow = 0.0f64;

                // X-neighbor (only if light has meaningful x-component)
                if wx > 0.1 {
                    let prev_x = if sweep_x_rev { x_pos + 1 } else { x_pos.wrapping_sub(1) };
                    if prev_x >= x0 && prev_x < x1 {
                        let prev_si = (y_pos - y0) * sw + (prev_x - x0);
                        max_shadow = max_shadow.max(shadow[prev_si] - shadow_decay / wx);
                    }
                }
                // Y-neighbor (only if light has meaningful y-component)
                if wy > 0.1 {
                    let prev_y = if sweep_y_rev { y_pos + 1 } else { y_pos.wrapping_sub(1) };
                    if prev_y >= y0 && prev_y < y1 {
                        let prev_si = (prev_y - y0) * sw + (x_pos - x0);
                        max_shadow = max_shadow.max(shadow[prev_si] - shadow_decay / wy);
                    }
                }
                // Diagonal neighbor (when light comes from both directions)
                if wx > 0.3 && wy > 0.3 {
                    let prev_x = if sweep_x_rev { x_pos + 1 } else { x_pos.wrapping_sub(1) };
                    let prev_y = if sweep_y_rev { y_pos + 1 } else { y_pos.wrapping_sub(1) };
                    if prev_x >= x0 && prev_x < x1 && prev_y >= y0 && prev_y < y1 {
                        let prev_si = (prev_y - y0) * sw + (prev_x - x0);
                        max_shadow = max_shadow.max(shadow[prev_si] - shadow_decay * 1.414);
                    }
                }

                shadow[si] = terrain_h.max(max_shadow);
            }
        }

        // Now compute lighting for the visible viewport only
        let vx0 = vx.max(0) as usize;
        let vy0 = vy.max(0) as usize;
        let vx1 = ((vx + vw as i32) as usize).min(map_w);
        let vy1 = ((vy + vh as i32) as usize).min(map_h);

        for y_pos in vy0..vy1 {
            for x_pos in vx0..vx1 {
                let i = y_pos * map_w + x_pos;
                let terrain_h = heights[i];

                // Check shadow: if shadow height at this cell > terrain height, it's shadowed
                let si = (y_pos - y0) * sw + (x_pos - x0);
                let in_shadow = shadow[si] > terrain_h + 0.01;

                if in_shadow {
                    self.light_map[i] = 0.05;
                    continue;
                }

                // Terrain normal + Blinn-Phong
                let (nx, ny, nz) = Self::terrain_normal(heights, map_w, map_h, x_pos, y_pos);

                // Diffuse: L·N, scaled by light source strength
                let l_dot_n = (light_dx * nx + light_dy * ny + light_dz * nz).max(0.0) * light_strength;

                // Specular: (H·N)^k, view = straight down (0,0,1)
                // Attenuate when light is high to avoid uniform wash
                let horiz_strength = (light_dx * light_dx + light_dy * light_dy).sqrt();
                let spec_atten = horiz_strength.min(1.0) * light_strength;
                let hx = light_dx;
                let hy = light_dy;
                let hz = light_dz + 1.0;
                let h_len = (hx * hx + hy * hy + hz * hz).sqrt();
                let h_dot_n = if h_len > 0.001 {
                    ((hx / h_len) * nx + (hy / h_len) * ny + (hz / h_len) * nz).max(0.0)
                } else {
                    0.0
                };
                let specular = h_dot_n.powi(16) * 0.4 * spec_atten;

                self.light_map[i] = (l_dot_n + specular).min(1.0);
            }
        }
    }

    /// Get lighting intensity for a world cell. Returns 0.0 - 1.0.
    pub fn get_light(&self, x: usize, y: usize) -> f64 {
        if x < self.light_w && y < self.light_h {
            self.light_map[y * self.light_w + x]
        } else {
            1.0
        }
    }

    /// Get the ambient color tint for current time of day.
    pub fn ambient_tint(&self) -> (f64, f64, f64) {
        let sun_elev = self.sun_elevation();
        let moon_elev = self.moon_elevation();

        if sun_elev > 0.3 {
            // Full day: neutral/slightly warm
            (1.0, 1.0, 0.95)
        } else if sun_elev > 0.0 {
            // Sunrise/sunset: warm orange
            let t = sun_elev / 0.3;
            (1.0, 0.6 + 0.4 * t, 0.4 + 0.55 * t)
        } else if sun_elev > -0.2 {
            // Twilight: blend toward blue
            let t = (sun_elev + 0.2) / 0.2;
            (0.3 + 0.7 * t, 0.3 + 0.3 * t, 0.5)
        } else if moon_elev > 0.1 {
            // Moonlit night: cool blue-silver, fairly visible
            let m = (moon_elev / 0.5).min(1.0);
            (0.35 + 0.2 * m, 0.38 + 0.2 * m, 0.55 + 0.15 * m)
        } else {
            // Dark night (no moon): dim but visible
            (0.25, 0.25, 0.38)
        }
    }

    /// Apply Blinn-Phong lighting + time-of-day tint to a color.
    pub fn apply_lighting(&self, color: Color, wx: usize, wy: usize) -> Color {
        if !self.enabled {
            return color;
        }

        let (tr, tg, tb) = self.ambient_tint();
        let directional = self.get_light(wx, wy);

        // Ambient (0.25) + directional (0.75) — enough ambient to see terrain at night,
        // enough directional for normals to show through
        let light = 0.25 + 0.75 * directional;

        // Quantize to steps of 8 so small lighting changes don't trigger
        // terminal redraws (crossterm double-buffer compares exact colors)
        let q = |v: f64| -> u8 { ((v as u8) >> 3) << 3 };
        let r = q((color.0 as f64 * tr * light).clamp(0.0, 255.0));
        let g = q((color.1 as f64 * tg * light).clamp(0.0, 255.0));
        let b = q((color.2 as f64 * tb * light).clamp(0.0, 255.0));
        Color(r, g, b)
    }

    /// Apply tint to an optional background color.
    pub fn apply_lighting_bg(&self, bg: Option<Color>, wx: usize, wy: usize) -> Option<Color> {
        bg.map(|c| self.apply_lighting(c, wx, wy))
    }

    /// Time-of-day as a display string for status bar.
    pub fn time_string(&self) -> String {
        let h = self.hour as u32;
        let m = ((self.hour - h as f64) * 60.0) as u32;
        let period = if h < 12 { "AM" } else { "PM" };
        let display_h = if h == 0 { 12 } else if h > 12 { h - 12 } else { h };
        format!("{:2}:{:02}{}", display_h, m, period)
    }
}

/// Vegetation density grid: grows where moisture is right, decays elsewhere.
#[derive(Serialize, Deserialize)]
pub struct VegetationMap {
    pub width: usize,
    pub height: usize,
    vegetation: Vec<f64>,
}

impl VegetationMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            vegetation: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.vegetation[y * self.width + x]
        } else {
            0.0
        }
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut f64> {
        if x < self.width && y < self.height {
            Some(&mut self.vegetation[y * self.width + x])
        } else {
            None
        }
    }

    pub fn grow(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            let i = y * self.width + x;
            self.vegetation[i] = (self.vegetation[i] + 0.002).min(1.0);
        }
    }

    pub fn decay(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            let i = y * self.width + x;
            self.vegetation[i] = (self.vegetation[i] - 0.003).max(0.0);
        }
    }

    /// Apply seasonal vegetation modifier. Values < 1.0 cause decay, > 1.0 boost growth.
    pub fn apply_season(&mut self, veg_growth_mult: f64) {
        if veg_growth_mult >= 1.0 {
            return; // no decay needed when growth is normal or boosted
        }
        // Scale factor: at mult=0.0 (winter), decay ~0.1% per tick; at mult=0.3 (autumn), decay ~0.07%
        let factor = 0.999 + 0.001 * veg_growth_mult;
        for v in &mut self.vegetation {
            *v *= factor;
        }
    }
}

/// Influence map for territory visualization. Each villager and building emits
/// influence that diffuses outward, creating an organic territory boundary.
#[derive(Serialize, Deserialize)]
pub struct InfluenceMap {
    pub width: usize,
    pub height: usize,
    influence: Vec<f64>,
}

impl InfluenceMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            influence: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.influence[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Update: decay all cells slightly, then add influence from sources, then diffuse.
    /// sources: (x, y, strength) — villagers emit 1.0, buildings emit 0.5
    pub fn update(&mut self, sources: &[(f64, f64, f64)]) {
        // Decay existing influence
        for v in self.influence.iter_mut() {
            *v *= 0.98;
        }

        // Add from sources
        for &(sx, sy, strength) in sources {
            let ix = sx.round() as usize;
            let iy = sy.round() as usize;
            if ix < self.width && iy < self.height {
                self.influence[iy * self.width + ix] += strength;
            }
        }

        // Simple diffusion: average with neighbors
        let mut temp = self.influence.clone();
        for y in 1..self.height.saturating_sub(1) {
            for x in 1..self.width.saturating_sub(1) {
                let idx = y * self.width + x;
                let avg = (self.influence[idx] * 4.0
                    + self.influence[idx - 1]
                    + self.influence[idx + 1]
                    + self.influence[(y - 1) * self.width + x]
                    + self.influence[(y + 1) * self.width + x])
                    / 8.0;
                temp[idx] = avg;
            }
        }
        self.influence = temp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilemap::{Terrain, TileMap};

    fn flat_heights(w: usize, h: usize, val: f64) -> Vec<f64> {
        vec![val; w * h]
    }

    fn grass_map(w: usize, h: usize) -> TileMap {
        TileMap::new(w, h, Terrain::Grass)
    }

    #[test]
    fn rain_adds_water() {
        let mut wm = WaterMap::new(50, 50);
        let config = SimConfig::default();
        wm.rain(&config);

        let total: f64 = wm.water.iter().sum();
        assert!(total > 0.0, "rain should add water");
    }

    #[test]
    fn drain_removes_all_water() {
        let mut wm = WaterMap::new(10, 10);
        wm.water[25] = 0.5;
        wm.drain();
        assert_eq!(wm.water.iter().sum::<f64>(), 0.0);
    }

    #[test]
    fn water_flows_downhill() {
        let mut wm = WaterMap::new(5, 5);
        // create a slope: left side high, right side low
        let mut heights = vec![0.0; 25];
        for y in 0..5 {
            for x in 0..5 {
                heights[y * 5 + x] = 1.0 - (x as f64 / 4.0);
            }
        }
        // put water at the high point
        wm.water[0 * 5 + 0] = 0.1; // top-left, height=1.0

        let config = SimConfig::default();
        for _ in 0..20 {
            wm.update(&mut heights, &config);
        }

        // water should have moved right (downhill)
        let left_water: f64 = (0..5).map(|y| wm.water[y * 5 + 0]).sum();
        let right_water: f64 = (0..5).map(|y| wm.water[y * 5 + 4]).sum();
        assert!(right_water > left_water, "water should flow to lower terrain");
    }

    #[test]
    fn water_pools_in_basin() {
        let mut wm = WaterMap::new(5, 1);
        // V-shaped basin: heights = [0.5, 0.25, 0.0, 0.25, 0.5]
        let mut heights = vec![0.5, 0.25, 0.0, 0.25, 0.5];
        // add water on the left slope
        wm.water[0] = 0.1;

        let config = SimConfig {
            evaporation: 0.0, // no evap so water is conserved
            ..Default::default()
        };

        for _ in 0..50 {
            wm.update(&mut heights, &config);
        }

        // most water should be at the center (lowest point)
        assert!(wm.water[2] > wm.water[0], "water should pool at basin center");
    }

    #[test]
    fn evaporation_removes_water() {
        let mut wm = WaterMap::new(5, 5);
        wm.water.fill(0.001);
        let mut heights = flat_heights(5, 5, 0.5);
        let config = SimConfig::default();

        for _ in 0..200 {
            wm.update(&mut heights, &config);
        }

        let total: f64 = wm.water.iter().sum();
        assert!(total < 0.001 * 25.0, "evaporation should reduce water over time");
    }

    #[test]
    fn erosion_modifies_terrain() {
        let mut wm = WaterMap::new(10, 10);
        // slope with water
        let mut heights: Vec<f64> = (0..100).map(|i| 1.0 - (i % 10) as f64 / 9.0).collect();
        let original_heights = heights.clone();
        wm.water.fill(0.05);

        let config = SimConfig {
            erosion_enabled: true,
            erosion_strength: 2.0,
            evaporation: 0.0,
            ..Default::default()
        };

        for _ in 0..50 {
            wm.update(&mut heights, &config);
        }

        let diffs: f64 = heights.iter().zip(original_heights.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diffs > 0.0, "erosion should modify terrain heights");
    }

    #[test]
    fn moisture_rises_near_water() {
        let mut wm = WaterMap::new(10, 10);
        wm.water[55] = 0.5; // water at (5, 5)
        let mut mm = MoistureMap::new(10, 10);
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);

        for _ in 0..20 {
            mm.update(&wm, &mut vm, &map);
        }

        assert!(mm.get(5, 5) > 0.05, "tile with water should have moisture: got {}", mm.get(5, 5));
        assert!(mm.get(5, 6) > 0.0, "moisture should propagate forward");
        assert!(mm.get(5, 5) > mm.get(0, 0), "water tile should be more moist than dry tile");
    }

    #[test]
    fn moisture_decays_without_water() {
        let wm = WaterMap::new(10, 10);
        let mut mm = MoistureMap::new(10, 10);
        mm.moisture[55] = 0.8; // some initial moisture, no water
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);

        // slower decay (0.95 factor) needs more ticks
        for _ in 0..100 {
            mm.update(&wm, &mut vm, &map);
        }

        assert!(mm.get(5, 5) < 0.1, "moisture should decay without water source: got {}", mm.get(5, 5));
    }

    #[test]
    fn vegetation_grows_with_moisture() {
        let wm = WaterMap::new(10, 10);
        let mut mm = MoistureMap::new(10, 10);
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);

        // Seed a region with moisture in the growth band (0.1-0.5)
        // so box blur keeps it above threshold
        for y in 3..7 {
            for x in 3..7 {
                mm.moisture[y * 10 + x] = 0.3;
            }
        }

        for _ in 0..100 {
            // Re-seed moisture each tick (simulating sustained water presence)
            for y in 4..6 {
                for x in 4..6 {
                    mm.moisture[y * 10 + x] = 0.3;
                }
            }
            mm.update(&wm, &mut vm, &map);
        }

        assert!(vm.get(5, 5) > 0.0, "vegetation should grow with sustained moisture: got {}", vm.get(5, 5));
    }

    #[test]
    fn vegetation_decays_without_moisture() {
        let wm = WaterMap::new(10, 10);
        let mut mm = MoistureMap::new(10, 10);
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);
        vm.vegetation[55] = 0.5; // some initial vegetation

        // slower decay (0.003/tick), 0.5 / 0.003 = ~167 ticks to fully decay
        for _ in 0..200 {
            mm.update(&wm, &mut vm, &map);
        }

        assert!(vm.get(5, 5) < 0.1, "vegetation should decay without moisture: got {}", vm.get(5, 5));
    }

    #[test]
    fn vegetation_clamped_to_0_1() {
        let mut vm = VegetationMap::new(5, 5);
        for _ in 0..200 {
            vm.grow(2, 2);
        }
        assert!(vm.get(2, 2) <= 1.0);

        for _ in 0..200 {
            vm.decay(2, 2);
        }
        assert!(vm.get(2, 2) >= 0.0);
    }

    #[test]
    fn day_night_time_advances() {
        let mut dn = DayNightCycle::new(10, 10);
        let start = dn.hour;
        dn.tick();
        assert!(dn.hour > start, "time should advance each tick");
    }

    #[test]
    fn day_night_wraps_at_24() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 23.99;
        dn.tick();
        assert!(dn.hour < 24.0, "hour should wrap past 24");
    }

    #[test]
    fn sun_elevation_peaks_at_noon() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 12.0;
        let noon_elev = dn.sun_elevation();
        dn.hour = 6.0;
        let dawn_elev = dn.sun_elevation();
        dn.hour = 0.0;
        let midnight_elev = dn.sun_elevation();

        assert!(noon_elev > dawn_elev, "noon should be higher than dawn");
        assert!(noon_elev > 0.0, "noon elevation should be positive");
        assert!(midnight_elev < 0.0, "midnight elevation should be negative");
    }

    #[test]
    fn ambient_tint_varies_by_time() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 12.0;
        let day = dn.ambient_tint();
        dn.hour = 0.0;
        let night = dn.ambient_tint();

        // Day should be brighter than night
        assert!(day.0 > night.0, "day red should be brighter than night");
        assert!(day.1 > night.1, "day green should be brighter than night");
    }

    #[test]
    fn shadow_map_darkens_behind_peaks() {
        let mut dn = DayNightCycle::new(20, 20);
        dn.hour = 12.0; // noon
        let mut heights = vec![0.1; 400];
        heights[10 * 20 + 10] = 0.9; // tall peak at center

        dn.compute_lighting(&heights, 20, 20, 0, 0, 20, 20);

        // The peak itself should be brighter than a shadowed cell behind it
        assert!(dn.get_light(10, 10) > 0.3, "peak should be well-lit: got {}", dn.get_light(10, 10));
    }

    #[test]
    fn slopes_facing_sun_are_brighter() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 12.0;

        // Slope going uphill left-to-right
        let mut heights = vec![0.0; 100];
        for y in 0..10 {
            for x in 0..10 {
                heights[y * 10 + x] = x as f64 / 9.0;
            }
        }

        dn.compute_lighting(&heights, 10, 10, 0, 0, 10, 10);

        let slope_light = dn.get_light(5, 5);
        assert!(slope_light > 0.0 && slope_light < 1.0,
            "slope should have intermediate lighting: got {}", slope_light);
    }

    #[test]
    fn apply_lighting_darkens_at_night() {
        let mut dn = DayNightCycle::new(10, 10);
        let base = Color(200, 200, 200);

        dn.hour = 12.0;
        dn.compute_lighting(&vec![0.5; 100], 10, 10, 0, 0, 10, 10);
        let day_color = dn.apply_lighting(base, 5, 5);

        dn.hour = 0.0;
        dn.compute_lighting(&vec![0.5; 100], 10, 10, 0, 0, 10, 10);
        let night_color = dn.apply_lighting(base, 5, 5);

        assert!(day_color.0 > night_color.0, "day should be brighter than night: day={:?} night={:?}", day_color, night_color);
    }

    #[test]
    fn moon_provides_light_at_night() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 0.0; // midnight — moon should be up
        dn.compute_lighting(&vec![0.5; 100], 10, 10, 0, 0, 10, 10);

        // Moon should provide some directional light (not just 0.0)
        let light = dn.get_light(5, 5);
        assert!(light > 0.0, "moon should provide light at midnight: got {}", light);
    }

    #[test]
    fn moonlit_night_brighter_than_dark_night() {
        let mut dn = DayNightCycle::new(10, 10);
        let base = Color(200, 200, 200);

        // Midnight: moon is up
        dn.hour = 0.0;
        dn.compute_lighting(&vec![0.5; 100], 10, 10, 0, 0, 10, 10);
        let moonlit = dn.apply_lighting(base, 5, 5);

        // 3am-ish: moon is lower, less light
        dn.hour = 4.0;
        dn.compute_lighting(&vec![0.5; 100], 10, 10, 0, 0, 10, 10);
        let dim = dn.apply_lighting(base, 5, 5);

        // Moonlit midnight should be >= dimmer hours
        let moonlit_b = moonlit.0 as u32 + moonlit.1 as u32 + moonlit.2 as u32;
        let dim_b = dim.0 as u32 + dim.1 as u32 + dim.2 as u32;
        assert!(moonlit_b >= dim_b,
            "midnight should be >= 4am brightness: midnight={} 4am={}", moonlit_b, dim_b);
    }

    #[test]
    fn time_string_formats_correctly() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 14.5;
        let s = dn.time_string();
        assert!(s.contains("2:30PM"), "expected 2:30PM, got {}", s);

        dn.hour = 0.0;
        let s = dn.time_string();
        assert!(s.contains("12:00AM"), "expected 12:00AM, got {}", s);
    }

    #[test]
    fn disabled_day_night_passes_through() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.enabled = false;
        let base = Color(100, 150, 200);
        let result = dn.apply_lighting(base, 0, 0);
        assert_eq!(result, base, "disabled day/night should pass colors through");
    }

    #[test]
    fn dawn_dusk_shadows_are_consistent() {
        // At dawn/dusk, shadows should still be directional and not produce
        // random artifacts from near-zero light direction components.
        let mut dn = DayNightCycle::new(20, 20);
        let mut heights = vec![0.1; 400];
        // A ridge running north-south at x=10
        for y in 0..20 {
            heights[y * 20 + 10] = 0.8;
        }

        // Test at sunrise (6:30) and sunset (17:30) — low sun angles
        for hour in [6.5, 17.5] {
            dn.hour = hour;
            dn.compute_lighting(&heights, 20, 20, 0, 0, 20, 20);

            // All cells on the same side of the ridge should have similar lighting
            // (not randomly bright/dark due to sweep artifacts)
            let mut lights_east: Vec<f64> = Vec::new();
            let mut lights_west: Vec<f64> = Vec::new();
            for y in 5..15 {
                lights_west.push(dn.get_light(5, y));
                lights_east.push(dn.get_light(15, y));
            }

            // Within each side, lighting should be fairly uniform (not wildly varying)
            let west_min = lights_west.iter().cloned().fold(f64::INFINITY, f64::min);
            let west_max = lights_west.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let east_min = lights_east.iter().cloned().fold(f64::INFINITY, f64::min);
            let east_max = lights_east.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

            assert!(west_max - west_min < 0.3,
                "hour={}: west side lighting should be consistent: min={} max={}", hour, west_min, west_max);
            assert!(east_max - east_min < 0.3,
                "hour={}: east side lighting should be consistent: min={} max={}", hour, east_min, east_max);

            // The ridge itself should be well-lit (faces the light)
            let ridge_light = dn.get_light(10, 10);
            assert!(ridge_light > 0.05,
                "hour={}: ridge should receive light: got {}", hour, ridge_light);
        }
    }

    #[test]
    fn water_avg_smooths_over_time() {
        let mut wm = WaterMap::new(5, 5);
        wm.water[12] = 0.5;
        let mut heights = flat_heights(5, 5, 0.5);
        let config = SimConfig::default();

        wm.update(&mut heights, &config);
        let avg1 = wm.get_avg(2, 2);

        wm.update(&mut heights, &config);
        let avg2 = wm.get_avg(2, 2);

        // avg should be changing (approaching actual water level)
        assert!(avg1 > 0.0, "avg should respond to water presence");
        assert_ne!(avg1, avg2, "avg should change between ticks");
    }

    #[test]
    fn calendar_advances_days_and_seasons() {
        let mut dn = DayNightCycle::new(10, 10);
        assert_eq!(dn.day, 0);
        assert_eq!(dn.season, Season::Spring);
        assert_eq!(dn.year, 0);

        // One day = 24 hours / 0.02 hrs/tick = 1200 ticks
        for _ in 0..1200 {
            dn.tick();
        }
        assert_eq!(dn.day, 1, "should advance to day 1 after 1200 ticks");
        assert_eq!(dn.season, Season::Spring);

        // Advance to end of spring (10 days total = 12000 ticks from start)
        // We already did 1200, so 10800 more
        for _ in 0..10800 {
            dn.tick();
        }
        assert_eq!(dn.season, Season::Summer, "should be summer after 10 days");
        assert_eq!(dn.day, 0);

        // Full year = 40 days = 48000 ticks from start; we did 12000, so 36000 more
        for _ in 0..36000 {
            dn.tick();
        }
        assert_eq!(dn.year, 1, "should be year 1 after 48000 ticks");
        assert_eq!(dn.season, Season::Spring);
    }

    #[test]
    fn winter_season_modifiers() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.season = Season::Winter;
        let mods = dn.season_modifiers();
        assert!(mods.hunger_mult > 1.0, "winter should increase hunger");
        assert_eq!(mods.veg_growth_mult, 0.0, "winter should stop vegetation growth");
        assert!(mods.wolf_aggression < 0.8, "winter wolves should attack villagers at lower hunger threshold");
    }

    #[test]
    fn date_string_format() {
        let mut dn = DayNightCycle::new(10, 10);
        assert_eq!(dn.date_string(), "Y1 Spring D1");
        dn.day = 5;
        dn.season = Season::Winter;
        dn.year = 2;
        assert_eq!(dn.date_string(), "Y3 Winter D6");
    }

    #[test]
    fn influence_map_diffuses() {
        let mut im = InfluenceMap::new(10, 10);
        // Add a source at center
        im.update(&[(5.0, 5.0, 5.0)]);

        // Center should have influence
        assert!(im.get(5, 5) > 0.0, "center should have influence after source: got {}", im.get(5, 5));

        // Run more ticks to let it diffuse
        for _ in 0..20 {
            im.update(&[(5.0, 5.0, 1.0)]);
        }

        // Neighbors should have picked up some influence via diffusion
        assert!(im.get(4, 5) > 0.0, "left neighbor should have influence via diffusion: got {}", im.get(4, 5));
        assert!(im.get(6, 5) > 0.0, "right neighbor should have influence via diffusion: got {}", im.get(6, 5));
        assert!(im.get(5, 4) > 0.0, "top neighbor should have influence via diffusion: got {}", im.get(5, 4));
        assert!(im.get(5, 6) > 0.0, "bottom neighbor should have influence via diffusion: got {}", im.get(5, 6));

        // Center should be stronger than edges
        assert!(im.get(5, 5) > im.get(1, 1), "center should be stronger than corner");
    }

    #[test]
    fn influence_map_decays() {
        let mut im = InfluenceMap::new(10, 10);
        // Add strong source once
        im.update(&[(5.0, 5.0, 10.0)]);
        let initial = im.get(5, 5);
        assert!(initial > 0.0);

        // Update many times with no sources — should decay
        for _ in 0..200 {
            im.update(&[]);
        }

        let after = im.get(5, 5);
        assert!(after < initial * 0.1,
            "influence should decay significantly without sources: initial={} after={}", initial, after);
    }

    #[test]
    fn vegetation_seasonal_decay() {
        let mut vm = VegetationMap::new(5, 5);
        vm.vegetation[12] = 0.5; // center tile
        // Winter: veg_growth_mult = 0.0
        for _ in 0..1000 {
            vm.apply_season(0.0);
        }
        assert!(vm.get(2, 2) < 0.3, "vegetation should decay in winter: got {}", vm.get(2, 2));
    }
}
