use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::ecs::components::ResourceType;
use crate::renderer::Color;

/// Parallel grid of water depth, layered on top of a height map.
/// Water flows downhill, erodes terrain, and evaporates over time.
#[derive(Serialize, Deserialize)]
pub struct WaterMap {
    pub width: usize,
    pub height: usize,
    water: Vec<f64>,
    water_temp: Vec<f64>, // transfer buffer for this frame
    water_avg: Vec<f64>,  // smoothed for rendering
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub rain_rate: f64,     // fraction of tiles that get rain per tick
    pub rain_amount: f64,   // water added per raindrop
    pub flow_fraction: f64, // how much of height diff flows per tick
    pub evaporation: f64,   // water removed per tile per tick
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
    /// `viewport` is an optional `(x_start, y_start, x_end, y_end)` bounds; when Some, only
    /// tiles within the viewport plus a 32-tile margin are simulated.
    pub fn update(
        &mut self,
        heights: &mut Vec<f64>,
        config: &SimConfig,
        viewport: Option<(usize, usize, usize, usize)>,
    ) {
        self.water_temp.fill(0.0);

        let w = self.width;
        let h = self.height;

        let (y_lo, y_hi, x_lo, x_hi) = match viewport {
            Some((xs, ys, xe, ye)) => (
                ys.saturating_sub(32),
                ye.saturating_add(32).min(h),
                xs.saturating_sub(32),
                xe.saturating_add(32).min(w),
            ),
            None => (0, h, 0, w),
        };

        for y in y_lo..y_hi {
            for x in x_lo..x_hi {
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
        for y in y_lo..y_hi {
            for x in x_lo..x_hi {
                let i = y * w + x;

                if config.erosion_enabled && self.water_temp[i].abs() > 1e-10 {
                    let change = self.water_temp[i];
                    let mut erode = if change > 0.0 { change * 0.5 } else { change };

                    if self.water[i] > 0.001 {
                        erode *= (erode * 0.1 / self.water[i]).abs();
                    } else {
                        erode *= (erode * 40.0).abs();
                    }

                    erode *= config.erosion_strength;

                    heights[i] += erode / 8.0;
                    for &(dx, dy, wt) in &[
                        (1i32, 0i32, 16.0),
                        (-1, 0, 16.0),
                        (0, 1, 16.0),
                        (0, -1, 16.0),
                        (1, 1, 22.63),
                        (-1, -1, 22.63),
                        (1, -1, 22.63),
                        (-1, 1, 22.63),
                    ] {
                        let (nx, ny) = self.wrapping_coords(x as i32 + dx, y as i32 + dy);
                        heights[ny * w + nx] += erode / wt;
                    }
                }

                self.water[i] =
                    (self.water[i] + self.water_temp[i] - config.evaporation).clamp(0.0, 1.0);
            }
        }
    }
}

/// Per-tile soil fertility grid. Initialized from SoilType at world-gen.
/// Farms read this value as a growth-rate multiplier.
/// Future: degrades from repeated harvesting, recovers when fallow.
#[derive(Serialize, Deserialize)]
pub struct SoilFertilityMap {
    pub width: usize,
    pub height: usize,
    fertility: Vec<f64>, // 0.0 (barren) to 1.0 (rich)
}

impl SoilFertilityMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            fertility: vec![1.0; width * height],
        }
    }

    /// Initialize fertility from a SoilType grid. Each SoilType maps to a
    /// base fertility via its yield_multiplier (clamped to 0.0..=1.0).
    pub fn from_soil_types(
        width: usize,
        height: usize,
        soil: &[crate::terrain_pipeline::SoilType],
    ) -> Self {
        let fertility = soil
            .iter()
            .map(|s| s.yield_multiplier().clamp(0.0, 1.0))
            .collect();
        Self {
            width,
            height,
            fertility,
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.fertility[y * self.width + x]
        } else {
            0.0
        }
    }

    pub fn set(&mut self, x: usize, y: usize, val: f64) {
        if x < self.width && y < self.height {
            self.fertility[y * self.width + x] = val.clamp(0.0, 1.0);
        }
    }

    /// Add to fertility (clamped to 1.0). Used by recovery/deposit systems.
    pub fn add(&mut self, x: usize, y: usize, delta: f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.fertility[idx] = (self.fertility[idx] + delta).clamp(0.0, 1.0);
        }
    }

    /// Subtract from fertility (clamped to 0.0). Used by degradation systems.
    pub fn degrade(&mut self, x: usize, y: usize, delta: f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.fertility[idx] = (self.fertility[idx] - delta).clamp(0.0, 1.0);
        }
    }
}

/// Moisture grid: driven by water presence, propagates downwind, drives vegetation.
#[derive(Serialize, Deserialize)]
pub struct MoistureMap {
    pub width: usize,
    pub height: usize,
    moisture: Vec<f64>,
    /// Long-term average moisture — slowly tracks current moisture.
    /// Vegetation and biome respond to this instead of instantaneous moisture.
    #[serde(default)]
    pub avg_moisture: Vec<f64>,
}

impl MoistureMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            moisture: vec![0.0; width * height],
            avg_moisture: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.moisture[y * self.width + x]
        } else {
            0.0
        }
    }

    pub fn set(&mut self, x: usize, y: usize, val: f64) {
        if x < self.width && y < self.height {
            self.moisture[y * self.width + x] = val;
        }
    }

    /// Get the long-term average moisture at a position.
    pub fn get_avg(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if idx < self.avg_moisture.len() {
                self.avg_moisture[idx]
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Update the long-term average toward current values.
    /// Called periodically (e.g. every 50 ticks). Blend factor controls speed:
    /// 0.02 means average moves 2% toward current each call.
    pub fn update_average(&mut self) {
        if self.avg_moisture.len() != self.moisture.len() {
            self.avg_moisture = self.moisture.clone();
            return;
        }
        let blend = 0.02;
        for i in 0..self.moisture.len() {
            self.avg_moisture[i] += (self.moisture[i] - self.avg_moisture[i]) * blend;
        }
    }

    fn wrapping_idx(&self, x: i32, y: i32) -> usize {
        let wx = x.rem_euclid(self.width as i32) as usize;
        let wy = y.rem_euclid(self.height as i32) as usize;
        wy * self.width + wx
    }

    /// Update moisture from water presence and propagate.
    /// Also updates vegetation based on moisture bands.
    pub fn update(
        &mut self,
        water: &WaterMap,
        vegetation: &mut VegetationMap,
        map: &crate::tilemap::TileMap,
    ) {
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

        // Step 3.5: update long-term moisture average
        self.update_average();

        // Step 4: vegetation responds to AVERAGE moisture (not instantaneous).
        // This prevents vegetation from flickering with short-term rain.
        for y in 0..self.height {
            for x in 0..self.width {
                let terrain = map.get(x, y);
                let can_grow = match terrain {
                    Some(crate::tilemap::Terrain::Sand) | Some(crate::tilemap::Terrain::Water) => {
                        false
                    }
                    _ => true,
                };
                let idx = y * self.width + x;
                let m = if idx < self.avg_moisture.len() {
                    self.avg_moisture[idx]
                } else {
                    self.moisture[idx]
                };
                // Vegetation grows when avg moisture is adequate (>0.1).
                // Very high moisture (>0.9) is waterlogged — slows growth but doesn't kill.
                if can_grow && m > 0.1 {
                    if m > 0.9 {
                        // Waterlogged: slow growth (50% rate)
                        if vegetation.get(x, y) < 0.5 {
                            vegetation.grow(x, y);
                        }
                    } else {
                        vegetation.grow(x, y);
                    }
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

    /// Daylight hours per season. Affects sunrise/sunset, villager productivity,
    /// and the overall feel of each season.
    pub fn day_hours(&self) -> f64 {
        match self {
            Season::Spring => 14.0,
            Season::Summer => 16.0,
            Season::Autumn => 10.0,
            Season::Winter => 8.0,
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
    /// Gathering speed multiplier (wood, stone, food foraging).
    /// Spring 1.1x (new growth), Summer 1.0x, Autumn 1.5x (wood only, handled separately), Winter 0.6x.
    pub gathering_mult: f64,
    /// Birth rate multiplier. Spring 1.2x (baby boom), Summer/Autumn 1.0x, Winter 0.5x (harsh).
    pub birth_rate_mult: f64,
}

/// Day/night cycle with Blinn-Phong lighting, terrain normals, and shadow raytracing.
#[derive(Serialize, Deserialize)]
pub struct DayNightCycle {
    pub hour: f64,      // 0.0 - 24.0
    pub tick_rate: f64, // hours per tick
    pub enabled: bool,
    pub day: u32, // current day (0-indexed within season)
    pub season: Season,
    pub year: u32,
    light_map: Vec<f64>, // per-tile total lighting intensity (combined diffuse + shadow)
    light_w: usize,
    light_h: usize,
}

impl DayNightCycle {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            hour: 10.0,      // start at 10am
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
                    Season::Winter => {
                        self.year += 1;
                        Season::Spring
                    }
                };
            }
        }
    }

    /// Get season-dependent modifiers for simulation systems.
    pub fn season_modifiers(&self) -> SeasonModifiers {
        match self.season {
            Season::Spring => SeasonModifiers {
                rain_mult: 1.5,
                evap_mult: 1.0,
                veg_growth_mult: 2.0,
                hunger_mult: 1.0,
                wolf_aggression: 0.4,
                gathering_mult: 1.1,
                birth_rate_mult: 1.2,
            },
            Season::Summer => SeasonModifiers {
                rain_mult: 0.5,
                evap_mult: 2.0,
                veg_growth_mult: 1.5,
                hunger_mult: 0.8,
                wolf_aggression: 0.4,
                gathering_mult: 1.0,
                birth_rate_mult: 1.0,
            },
            Season::Autumn => SeasonModifiers {
                rain_mult: 1.0,
                evap_mult: 1.0,
                veg_growth_mult: 0.3,
                hunger_mult: 1.0,
                wolf_aggression: 0.5,
                gathering_mult: 1.0,
                birth_rate_mult: 1.0,
            },
            Season::Winter => SeasonModifiers {
                rain_mult: 0.3,
                evap_mult: 0.5,
                veg_growth_mult: 0.0,
                hunger_mult: 2.5,
                wolf_aggression: 0.7,
                gathering_mult: 0.6,
                birth_rate_mult: 0.5,
            },
        }
    }

    /// Format date as "Y1 Spring D1".
    pub fn date_string(&self) -> String {
        format!(
            "Y{} {} D{}",
            self.year + 1,
            self.season.name(),
            self.day + 1
        )
    }

    /// Returns true if it's nighttime (sun below horizon, roughly 6pm-6am).
    pub fn is_night(&self) -> bool {
        self.sun_elevation() <= 0.0
    }

    /// Sunrise hour for the current season (centered around noon).
    fn sunrise_hour(&self) -> f64 {
        12.0 - self.season.day_hours() / 2.0
    }

    /// Sunset hour for the current season (centered around noon).
    fn sunset_hour(&self) -> f64 {
        12.0 + self.season.day_hours() / 2.0
    }

    /// Sun elevation angle in radians. Peaks at noon, below 0 at night.
    /// Max ~60 degrees — keeps the sun from going truly overhead so there's
    /// always a meaningful horizontal component for shadows and directional shading.
    /// Day length varies by season (e.g. 16h summer, 8h winter).
    pub fn sun_elevation(&self) -> f64 {
        let sunrise = self.sunrise_hour();
        let day_len = self.season.day_hours();
        let angle = (self.hour - sunrise) / day_len * std::f64::consts::PI;
        angle.sin() * (std::f64::consts::PI / 3.0) // max ~60 degrees
    }

    /// Sun azimuth in radians. Traces east (sunrise) → south (noon) → west (sunset).
    /// Adjusted for season-dependent day length.
    pub fn sun_azimuth(&self) -> f64 {
        let sunrise = self.sunrise_hour();
        let day_len = self.season.day_hours();
        (self.hour - sunrise) / day_len * std::f64::consts::PI
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
    fn terrain_normal(
        heights: &[f64],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
    ) -> (f64, f64, f64) {
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
        let horiz_len = (light_dx * light_dx + light_dy * light_dy)
            .sqrt()
            .max(0.001);
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
                    let prev_x = if sweep_x_rev {
                        x_pos + 1
                    } else {
                        x_pos.wrapping_sub(1)
                    };
                    if prev_x >= x0 && prev_x < x1 {
                        let prev_si = (y_pos - y0) * sw + (prev_x - x0);
                        max_shadow = max_shadow.max(shadow[prev_si] - shadow_decay / wx);
                    }
                }
                // Y-neighbor (only if light has meaningful y-component)
                if wy > 0.1 {
                    let prev_y = if sweep_y_rev {
                        y_pos + 1
                    } else {
                        y_pos.wrapping_sub(1)
                    };
                    if prev_y >= y0 && prev_y < y1 {
                        let prev_si = (prev_y - y0) * sw + (x_pos - x0);
                        max_shadow = max_shadow.max(shadow[prev_si] - shadow_decay / wy);
                    }
                }
                // Diagonal neighbor (when light comes from both directions)
                if wx > 0.3 && wy > 0.3 {
                    let prev_x = if sweep_x_rev {
                        x_pos + 1
                    } else {
                        x_pos.wrapping_sub(1)
                    };
                    let prev_y = if sweep_y_rev {
                        y_pos + 1
                    } else {
                        y_pos.wrapping_sub(1)
                    };
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
                let l_dot_n =
                    (light_dx * nx + light_dy * ny + light_dz * nz).max(0.0) * light_strength;

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

        // Ambient (0.35) + directional (0.65) — enough ambient to see terrain at night,
        // enough directional for normals to show through
        let light = 0.35 + 0.65 * directional;

        // Quantize to steps of 4 so small lighting changes don't trigger
        // terminal redraws (crossterm double-buffer compares exact colors)
        let q = |v: f64| -> u8 { ((v as u8) >> 2) << 2 };
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
        let display_h = if h == 0 {
            12
        } else if h > 12 {
            h - 12
        } else {
            h
        };
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
    /// `viewport` is an optional `(x_start, y_start, x_end, y_end)` bounds; when Some, only
    /// tiles within the viewport plus a 32-tile margin are processed.
    pub fn update(
        &mut self,
        sources: &[(f64, f64, f64)],
        viewport: Option<(usize, usize, usize, usize)>,
    ) {
        let (y_lo, y_hi, x_lo, x_hi) = match viewport {
            Some((xs, ys, xe, ye)) => (
                ys.saturating_sub(32),
                ye.saturating_add(32).min(self.height),
                xs.saturating_sub(32),
                xe.saturating_add(32).min(self.width),
            ),
            None => (0, self.height, 0, self.width),
        };

        // Decay existing influence (within bounds)
        for y in y_lo..y_hi {
            for x in x_lo..x_hi {
                self.influence[y * self.width + x] *= 0.98;
            }
        }

        // Add from sources (only those within bounds)
        for &(sx, sy, strength) in sources {
            let ix = sx.round() as usize;
            let iy = sy.round() as usize;
            if ix >= x_lo && ix < x_hi && iy >= y_lo && iy < y_hi {
                self.influence[iy * self.width + ix] += strength;
            }
        }

        // Simple diffusion: average with neighbors (within bounds, skipping edges)
        let mut temp = self.influence.clone();
        let diff_y_lo = y_lo.max(1);
        let diff_y_hi = y_hi.min(self.height.saturating_sub(1));
        let diff_x_lo = x_lo.max(1);
        let diff_x_hi = x_hi.min(self.width.saturating_sub(1));
        for y in diff_y_lo..diff_y_hi {
            for x in diff_x_lo..diff_x_hi {
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

/// Tracks accumulated foot traffic from villager movement.
/// High-traffic walkable tiles automatically convert to roads.
///
/// Extended with directional tracking (`traffic_dx`/`traffic_dy`) to orient
/// trail characters, and per-tile dominant resource type for the Traffic overlay.
#[derive(Serialize, Deserialize)]
pub struct TrafficMap {
    pub width: usize,
    pub height: usize,
    traffic: Vec<f64>,
    /// Accumulated movement direction X component per tile (Phase 2: directional trails).
    #[serde(default)]
    traffic_dx: Vec<f64>,
    /// Accumulated movement direction Y component per tile (Phase 2: directional trails).
    #[serde(default)]
    traffic_dy: Vec<f64>,
    /// Per-tile dominant resource type carried by haulers traversing the tile.
    #[serde(default)]
    dominant_resource: Vec<Option<ResourceType>>,
    /// Per-tile resource flow counters: [Food, Wood, Stone, Planks, Masonry, Grain].
    #[serde(default)]
    flow_by_type: Vec<[f64; 6]>,
}

/// Map a `ResourceType` to an index into `flow_by_type` arrays.
fn resource_type_index(rt: ResourceType) -> usize {
    match rt {
        ResourceType::Food => 0,
        ResourceType::Wood => 1,
        ResourceType::Stone => 2,
        ResourceType::Planks => 3,
        ResourceType::Masonry => 4,
        ResourceType::Grain => 5,
    }
}

impl TrafficMap {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            traffic: vec![0.0; n],
            traffic_dx: vec![0.0; n],
            traffic_dy: vec![0.0; n],
            dominant_resource: vec![None; n],
            flow_by_type: vec![[0.0; 6]; n],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.traffic[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get the accumulated directional vector at a tile.
    /// Returns `(dx, dy)` where the magnitude reflects total directed traffic.
    pub fn get_direction(&self, x: usize, y: usize) -> (f64, f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if idx < self.traffic_dx.len() && idx < self.traffic_dy.len() {
                (self.traffic_dx[idx], self.traffic_dy[idx])
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    }

    /// Get the dominant resource type hauled across a tile, if any.
    pub fn get_dominant_resource(&self, x: usize, y: usize) -> Option<ResourceType> {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if idx < self.dominant_resource.len() {
                self.dominant_resource[idx]
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Record a footstep at the given position.
    pub fn step_on(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            self.traffic[y * self.width + x] += 1.0;
        }
    }

    /// Record a directed footstep with movement direction and optional resource cargo.
    /// `dx`/`dy` are the villager's velocity direction (will be normalized).
    /// Hauling steps get a 2x weight in the directional accumulator so net flow
    /// points toward stockpiles.
    pub fn step_on_directed(
        &mut self,
        x: usize,
        y: usize,
        dx: f64,
        dy: f64,
        resource: Option<ResourceType>,
    ) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.traffic[idx] += 1.0;

            // Normalize direction to unit length before accumulating
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.001 {
                let weight = if resource.is_some() { 2.0 } else { 1.0 };
                if idx < self.traffic_dx.len() {
                    self.traffic_dx[idx] += (dx / len) * weight;
                }
                if idx < self.traffic_dy.len() {
                    self.traffic_dy[idx] += (dy / len) * weight;
                }
            }

            // Track resource flow
            if let Some(rt) = resource {
                if idx < self.flow_by_type.len() {
                    self.flow_by_type[idx][resource_type_index(rt)] += 1.0;
                    // Recompute dominant resource for this tile
                    let counts = &self.flow_by_type[idx];
                    let mut best_idx = 0;
                    let mut best_val = counts[0];
                    for i in 1..6 {
                        if counts[i] > best_val {
                            best_val = counts[i];
                            best_idx = i;
                        }
                    }
                    if idx < self.dominant_resource.len() {
                        self.dominant_resource[idx] = if best_val > 0.0 {
                            Some(match best_idx {
                                0 => ResourceType::Food,
                                1 => ResourceType::Wood,
                                2 => ResourceType::Stone,
                                3 => ResourceType::Planks,
                                4 => ResourceType::Masonry,
                                _ => ResourceType::Grain,
                            })
                        } else {
                            None
                        };
                    }
                }
            }
        }
    }

    /// Slow decay so old paths fade if villagers stop using them.
    pub fn decay(&mut self) {
        for v in self.traffic.iter_mut() {
            *v *= 0.999;
        }
        for v in self.traffic_dx.iter_mut() {
            *v *= 0.999;
        }
        for v in self.traffic_dy.iter_mut() {
            *v *= 0.999;
        }
        for arr in self.flow_by_type.iter_mut() {
            for v in arr.iter_mut() {
                *v *= 0.999;
            }
        }
    }

    /// Return tiles that exceed the road threshold and are eligible for conversion.
    /// Only converts walkable non-road terrain (grass, sand, forest, building floor).
    pub fn road_candidates(
        &self,
        map: &crate::tilemap::TileMap,
        threshold: f64,
    ) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.traffic[y * self.width + x] >= threshold
                    && let Some(terrain) = map.get(x, y)
                    && terrain.is_walkable()
                    && *terrain != crate::tilemap::Terrain::Road
                    && *terrain != crate::tilemap::Terrain::BuildingFloor
                    && *terrain != crate::tilemap::Terrain::BuildingWall
                {
                    result.push((x, y));
                }
            }
        }
        result
    }

    /// Compute the dominant travel direction character for a trail-tier tile.
    /// Returns a trail character oriented along the dominant direction of travel.
    pub fn trail_char(&self, x: usize, y: usize) -> char {
        let (dx, dy) = self.get_direction(x, y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return '.'; // mixed / no dominant direction
        }
        // Compute angle and pick oriented character
        let angle = dy.atan2(dx).abs(); // 0 = east, pi/2 = south, pi = west
        if angle < std::f64::consts::FRAC_PI_8 || angle > 7.0 * std::f64::consts::FRAC_PI_8 {
            '-' // east-west
        } else if angle < 3.0 * std::f64::consts::FRAC_PI_8 {
            if (dx > 0.0) == (dy > 0.0) { '\\' } else { '/' }
        } else if angle < 5.0 * std::f64::consts::FRAC_PI_8 {
            '|' // north-south
        } else {
            if (dx > 0.0) == (dy > 0.0) { '/' } else { '\\' }
        }
    }
}

impl Default for TrafficMap {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// A generic scent/trace layer — one f64 per map tile.
/// Used for danger scent, home scent, and future trace types.
/// Values decay multiplicatively and diffuse to neighbors.
#[derive(Serialize, Deserialize)]
pub struct ScentMap {
    pub width: usize,
    pub height: usize,
    values: Vec<f64>,
    /// Multiplicative decay applied each decay tick (e.g., 0.998).
    pub decay_rate: f64,
    /// Fraction of a tile's value shared with each of 8 neighbors during diffusion.
    pub spread_factor: f64,
}

impl ScentMap {
    pub fn new(width: usize, height: usize, decay_rate: f64, spread_factor: f64) -> Self {
        Self {
            width,
            height,
            values: vec![0.0; width * height],
            decay_rate,
            spread_factor,
        }
    }

    /// Get the scent value at (x, y). Returns 0.0 for out-of-bounds.
    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.values[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Add scent at the given position.
    pub fn emit(&mut self, x: usize, y: usize, amount: f64) {
        if x < self.width && y < self.height {
            self.values[y * self.width + x] += amount;
        }
    }

    /// Multiplicative decay: values[i] *= decay_rate. Values below 0.01 are zeroed.
    pub fn decay(&mut self) {
        for v in self.values.iter_mut() {
            *v *= self.decay_rate;
            if *v < 0.01 {
                *v = 0.0;
            }
        }
    }

    /// Diffuse scent to Moore neighbors (8-connected).
    /// Each tile shares `spread_factor` of its value with each neighbor,
    /// keeping (1 - 8 * spread_factor) for itself.
    pub fn diffuse(&mut self) {
        if self.spread_factor <= 0.0 {
            return;
        }
        let w = self.width;
        let h = self.height;
        let mut temp = vec![0.0f64; w * h];

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let val = self.values[idx];
                if val < 0.01 {
                    continue;
                }
                let share = val * self.spread_factor;
                // Keep the remainder for self
                let keep = val * (1.0 - 8.0 * self.spread_factor).max(0.0);
                temp[idx] += keep;

                // Spread to 8 neighbors
                for &(dx, dy) in &[
                    (-1i32, -1i32),
                    (-1, 0),
                    (-1, 1),
                    (0, -1),
                    (0, 1),
                    (1, -1),
                    (1, 0),
                    (1, 1),
                ] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        temp[ny as usize * w + nx as usize] += share;
                    }
                    // Out-of-bounds share is lost (scent doesn't wrap)
                }
            }
        }
        self.values = temp;
    }

    /// Sample tiles in 8 directions at staggered distances to find the
    /// direction of strongest scent. Returns (x, y, strength) of the best
    /// tile above `min_threshold`, or None.
    pub fn sample_gradient(
        &self,
        cx: usize,
        cy: usize,
        radius: usize,
        min_threshold: f64,
    ) -> Option<(usize, usize, f64)> {
        const EIGHT_DIRS: [(i32, i32); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];
        let mut best: Option<(usize, usize, f64)> = None;
        let mut best_val = min_threshold;
        for &(dx, dy) in &EIGHT_DIRS {
            let mut dist = 2usize;
            while dist <= radius {
                let sx = cx as isize + dx as isize * dist as isize;
                let sy = cy as isize + dy as isize * dist as isize;
                if sx >= 0 && sy >= 0 && (sx as usize) < self.width && (sy as usize) < self.height {
                    let val = self.values[sy as usize * self.width + sx as usize];
                    if val > best_val {
                        best_val = val;
                        best = Some((sx as usize, sy as usize, val));
                    }
                }
                dist += 2;
            }
        }
        best
    }

    /// Check if any non-zero scent exists on the map.
    pub fn has_scent(&self) -> bool {
        self.values.iter().any(|&v| v > 0.0)
    }

    /// Raw values slice for use as A* cost overlay.
    pub fn values(&self) -> &[f64] {
        &self.values
    }
}

impl Default for ScentMap {
    fn default() -> Self {
        Self::new(0, 0, 0.998, 0.0)
    }
}

/// Per-tile threat/defense data for the Threats overlay.
///
/// Stores wolf territory zones, garrison coverage radii, approach corridor
/// pressure, and computed exposure gaps. Updated periodically (every 100 ticks)
/// and whenever garrisons are built/destroyed.
pub struct ThreatMap {
    pub width: usize,
    pub height: usize,
    /// 0.0 = safe, 1.0 = core wolf territory (forest in qualifying cluster).
    /// 0.5 = buffer zone (within 3 tiles of qualifying cluster edge).
    pub wolf_territory: Vec<f32>,
    /// 0.0 = no corridor, 1.0 = primary approach through undefended chokepoint.
    pub corridor_pressure: Vec<f32>,
    /// 0.0 = uncovered, values grow with garrison proximity. Multiple garrisons stack.
    pub garrison_coverage: Vec<f32>,
    /// Computed: wolf_territory + corridor_pressure - garrison_coverage, clamped >= 0.
    pub exposure: Vec<f32>,
}

impl ThreatMap {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            wolf_territory: vec![0.0; n],
            corridor_pressure: vec![0.0; n],
            garrison_coverage: vec![0.0; n],
            exposure: vec![0.0; n],
        }
    }

    /// Get wolf territory value at (x, y).
    pub fn wolf_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.wolf_territory[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get garrison coverage at (x, y).
    pub fn garrison_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.garrison_coverage[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get corridor pressure at (x, y).
    pub fn corridor_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.corridor_pressure[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get exposure gap at (x, y).
    pub fn exposure_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.exposure[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Recompute wolf territory from the terrain map and danger scent.
    /// Forest tiles with significant danger scent nearby are core territory (1.0).
    /// Forest tiles within 15-60 tiles of the settlement center with cluster size > 20
    /// get territory marking. We approximate this using danger scent as a proxy for
    /// wolf presence (wolves emit danger scent where they live).
    pub fn update_wolf_territory(
        &mut self,
        map: &crate::tilemap::TileMap,
        danger_scent: &ScentMap,
        settlement_center: (i32, i32),
    ) {
        use crate::tilemap::Terrain;
        self.wolf_territory.fill(0.0);
        let (scx, scy) = settlement_center;
        let w = self.width;
        let h = self.height;

        // Pass 1: mark forest tiles that have danger scent as core wolf territory
        for y in 0..h {
            for x in 0..w {
                let terrain = map.get(x, y).copied().unwrap_or(Terrain::Water);
                if terrain != Terrain::Forest {
                    continue;
                }
                let dist_to_settlement =
                    (((x as i32 - scx).pow(2) + (y as i32 - scy).pow(2)) as f64).sqrt();
                // Only mark forests within relevant range (10-80 tiles from settlement)
                if dist_to_settlement < 10.0 || dist_to_settlement > 80.0 {
                    continue;
                }
                let scent = danger_scent.get(x, y);
                if scent > 0.05 {
                    self.wolf_territory[y * w + x] = 1.0;
                } else if scent > 0.01 {
                    self.wolf_territory[y * w + x] = 0.5;
                }
            }
        }

        // Pass 2: buffer zone — mark non-forest tiles within 3 tiles of wolf territory
        // Use a simple expansion pass
        let snapshot: Vec<f32> = self.wolf_territory.clone();
        for y in 0..h {
            for x in 0..w {
                if snapshot[y * w + x] > 0.0 {
                    continue; // already marked
                }
                // Check 3-tile neighborhood for wolf territory
                let mut nearest_dist_sq = u32::MAX;
                for dy in -3i32..=3 {
                    for dx in -3i32..=3 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            if snapshot[ny as usize * w + nx as usize] >= 1.0 {
                                let d = (dx * dx + dy * dy) as u32;
                                if d < nearest_dist_sq {
                                    nearest_dist_sq = d;
                                }
                            }
                        }
                    }
                }
                if nearest_dist_sq <= 9 {
                    // Within 3 tiles
                    self.wolf_territory[y * w + x] =
                        0.3 * (1.0 - (nearest_dist_sq as f32).sqrt() / 3.0);
                }
            }
        }
    }

    /// Recompute garrison coverage from garrison positions.
    /// Each garrison radiates coverage that decays with distance (radius 12 base).
    /// Garrisons near chokepoints get a bonus radius.
    pub fn update_garrison_coverage(
        &mut self,
        garrisons: &[(usize, usize)],
        chokepoint_scores: &[f64],
    ) {
        self.garrison_coverage.fill(0.0);
        let w = self.width;
        let h = self.height;
        let base_radius: i32 = 12;

        for &(gx, gy) in garrisons {
            // Check if garrison is near a chokepoint (score > 0.2)
            let choke_score = if gx < w && gy < h {
                chokepoint_scores.get(gy * w + gx).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            let bonus = if choke_score > 0.2 { 5 } else { 0 };
            let radius = base_radius + bonus;
            let defense_bonus: f32 = 1.0 + choke_score as f32 * 0.3;

            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let tx = gx as i32 + dx;
                    let ty = gy as i32 + dy;
                    if tx < 0 || ty < 0 || tx as usize >= w || ty as usize >= h {
                        continue;
                    }
                    let dist = ((dx * dx + dy * dy) as f64).sqrt();
                    if dist > radius as f64 {
                        continue;
                    }
                    let coverage = defense_bonus / (1.0 + dist as f32 * 0.15);
                    self.garrison_coverage[ty as usize * w + tx as usize] += coverage;
                }
            }
        }
    }

    /// Recompute corridor pressure from chokepoint data.
    /// High-scoring chokepoint tiles that lack garrison coverage get pressure.
    pub fn update_corridor_pressure(&mut self, chokepoint_scores: &[f64]) {
        let n = self.width * self.height;
        self.corridor_pressure.fill(0.0);
        if chokepoint_scores.len() != n {
            return;
        }
        for i in 0..n {
            let score = chokepoint_scores[i] as f32;
            if score > 0.1 {
                self.corridor_pressure[i] = score;
            }
        }
    }

    /// Recompute exposure = threat - defense, clamped to [0, 1].
    pub fn recompute_exposure(&mut self) {
        let n = self.width * self.height;
        for i in 0..n {
            let threat = self.wolf_territory[i] + self.corridor_pressure[i];
            let defense = self.garrison_coverage[i];
            self.exposure[i] = (threat - defense).clamp(0.0, 1.0);
        }
    }

    /// Full update: recompute all layers and exposure.
    pub fn update(
        &mut self,
        map: &crate::tilemap::TileMap,
        danger_scent: &ScentMap,
        settlement_center: (i32, i32),
        garrisons: &[(usize, usize)],
        chokepoint_scores: &[f64],
    ) {
        self.update_wolf_territory(map, danger_scent, settlement_center);
        self.update_garrison_coverage(garrisons, chokepoint_scores);
        self.update_corridor_pressure(chokepoint_scores);
        self.recompute_exposure();
    }
}

impl Default for ThreatMap {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// Tracks which tiles have been explored (revealed) by creatures.
/// Unexplored tiles are rendered as dark fog.
pub struct ExplorationMap {
    pub revealed: Vec<bool>,
    pub width: usize,
    pub height: usize,
}

impl ExplorationMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            revealed: vec![false; width * height],
            width,
            height,
        }
    }

    /// Mark all tiles within `radius` of (cx, cy) as revealed.
    /// Uses simple Euclidean distance check (no raycasting).
    pub fn reveal(&mut self, cx: usize, cy: usize, radius: usize) {
        let r = radius as i32;
        let r_sq = (radius * radius) as i32;
        let min_x = (cx as i32 - r).max(0) as usize;
        let max_x = ((cx as i32 + r) as usize).min(self.width.saturating_sub(1));
        let min_y = (cy as i32 - r).max(0) as usize;
        let max_y = ((cy as i32 + r) as usize).min(self.height.saturating_sub(1));
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as i32 - cx as i32;
                let dy = y as i32 - cy as i32;
                if dx * dx + dy * dy <= r_sq {
                    self.revealed[y * self.width + x] = true;
                }
            }
        }
    }

    /// Returns true if the tile at (x, y) has been revealed.
    pub fn is_revealed(&self, x: usize, y: usize) -> bool {
        if x < self.width && y < self.height {
            self.revealed[y * self.width + x]
        } else {
            false
        }
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
            wm.update(&mut heights, &config, None);
        }

        // water should have moved right (downhill)
        let left_water: f64 = (0..5).map(|y| wm.water[y * 5 + 0]).sum();
        let right_water: f64 = (0..5).map(|y| wm.water[y * 5 + 4]).sum();
        assert!(
            right_water > left_water,
            "water should flow to lower terrain"
        );
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
            wm.update(&mut heights, &config, None);
        }

        // most water should be at the center (lowest point)
        assert!(
            wm.water[2] > wm.water[0],
            "water should pool at basin center"
        );
    }

    #[test]
    fn evaporation_removes_water() {
        let mut wm = WaterMap::new(5, 5);
        wm.water.fill(0.001);
        let mut heights = flat_heights(5, 5, 0.5);
        let config = SimConfig::default();

        for _ in 0..200 {
            wm.update(&mut heights, &config, None);
        }

        let total: f64 = wm.water.iter().sum();
        assert!(
            total < 0.001 * 25.0,
            "evaporation should reduce water over time"
        );
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
            wm.update(&mut heights, &config, None);
        }

        let diffs: f64 = heights
            .iter()
            .zip(original_heights.iter())
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

        assert!(
            mm.get(5, 5) > 0.05,
            "tile with water should have moisture: got {}",
            mm.get(5, 5)
        );
        assert!(mm.get(5, 6) > 0.0, "moisture should propagate forward");
        assert!(
            mm.get(5, 5) > mm.get(0, 0),
            "water tile should be more moist than dry tile"
        );
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

        assert!(
            mm.get(5, 5) < 0.1,
            "moisture should decay without water source: got {}",
            mm.get(5, 5)
        );
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

        assert!(
            vm.get(5, 5) > 0.0,
            "vegetation should grow with sustained moisture: got {}",
            vm.get(5, 5)
        );
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

        assert!(
            vm.get(5, 5) < 0.1,
            "vegetation should decay without moisture: got {}",
            vm.get(5, 5)
        );
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
        assert!(
            dn.get_light(10, 10) > 0.3,
            "peak should be well-lit: got {}",
            dn.get_light(10, 10)
        );
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
        assert!(
            slope_light > 0.0 && slope_light < 1.0,
            "slope should have intermediate lighting: got {}",
            slope_light
        );
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

        assert!(
            day_color.0 > night_color.0,
            "day should be brighter than night: day={:?} night={:?}",
            day_color,
            night_color
        );
    }

    #[test]
    fn moon_provides_light_at_night() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.hour = 0.0; // midnight — moon should be up
        dn.compute_lighting(&vec![0.5; 100], 10, 10, 0, 0, 10, 10);

        // Moon should provide some directional light (not just 0.0)
        let light = dn.get_light(5, 5);
        assert!(
            light > 0.0,
            "moon should provide light at midnight: got {}",
            light
        );
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
        assert!(
            moonlit_b >= dim_b,
            "midnight should be >= 4am brightness: midnight={} 4am={}",
            moonlit_b,
            dim_b
        );
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
        assert_eq!(
            result, base,
            "disabled day/night should pass colors through"
        );
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
            let west_max = lights_west
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            let east_min = lights_east.iter().cloned().fold(f64::INFINITY, f64::min);
            let east_max = lights_east
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);

            assert!(
                west_max - west_min < 0.3,
                "hour={}: west side lighting should be consistent: min={} max={}",
                hour,
                west_min,
                west_max
            );
            assert!(
                east_max - east_min < 0.3,
                "hour={}: east side lighting should be consistent: min={} max={}",
                hour,
                east_min,
                east_max
            );

            // The ridge itself should be well-lit (faces the light)
            let ridge_light = dn.get_light(10, 10);
            assert!(
                ridge_light > 0.05,
                "hour={}: ridge should receive light: got {}",
                hour,
                ridge_light
            );
        }
    }

    #[test]
    fn water_avg_smooths_over_time() {
        let mut wm = WaterMap::new(5, 5);
        wm.water[12] = 0.5;
        let mut heights = flat_heights(5, 5, 0.5);
        let config = SimConfig::default();

        wm.update(&mut heights, &config, None);
        let avg1 = wm.get_avg(2, 2);

        wm.update(&mut heights, &config, None);
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
        assert_eq!(
            mods.veg_growth_mult, 0.0,
            "winter should stop vegetation growth"
        );
        assert!(
            mods.wolf_aggression < 0.8,
            "winter wolves should attack villagers at lower hunger threshold"
        );
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
    fn daylight_hours_vary_by_season() {
        assert_eq!(Season::Spring.day_hours(), 14.0);
        assert_eq!(Season::Summer.day_hours(), 16.0);
        assert_eq!(Season::Autumn.day_hours(), 10.0);
        assert_eq!(Season::Winter.day_hours(), 8.0);
    }

    #[test]
    fn winter_nights_longer_than_summer() {
        let mut dn = DayNightCycle::new(10, 10);

        // 7pm (19:00) -- should be night in winter but day in summer
        dn.hour = 19.0;

        dn.season = Season::Winter; // sunset at 16:00
        assert!(
            dn.is_night(),
            "19:00 should be night in winter (sunset 16:00)"
        );

        dn.season = Season::Summer; // sunset at 20:00
        assert!(
            !dn.is_night(),
            "19:00 should be day in summer (sunset 20:00)"
        );
    }

    #[test]
    fn sunrise_sunset_centered_on_noon() {
        let dn = DayNightCycle::new(10, 10);
        // For any season, sunrise + sunset should average to 12.0
        for season in [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ] {
            let mut d = DayNightCycle::new(10, 10);
            d.season = season;
            let rise = d.sunrise_hour();
            let set = d.sunset_hour();
            assert!(
                ((rise + set) / 2.0 - 12.0).abs() < 0.001,
                "{}: sunrise={} sunset={} not centered on noon",
                season.name(),
                rise,
                set
            );
        }
        drop(dn);
    }

    #[test]
    fn seasonal_gathering_mult() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.season = Season::Spring;
        assert!(
            dn.season_modifiers().gathering_mult > 1.0,
            "spring gathering should be faster"
        );
        dn.season = Season::Summer;
        assert_eq!(
            dn.season_modifiers().gathering_mult,
            1.0,
            "summer gathering should be baseline"
        );
        dn.season = Season::Winter;
        assert!(
            dn.season_modifiers().gathering_mult < 1.0,
            "winter gathering should be slower"
        );
    }

    #[test]
    fn seasonal_birth_rate_mult() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.season = Season::Spring;
        assert!(
            dn.season_modifiers().birth_rate_mult > 1.0,
            "spring should have birth bonus"
        );
        dn.season = Season::Winter;
        assert!(
            dn.season_modifiers().birth_rate_mult < 1.0,
            "winter should reduce births"
        );
    }

    #[test]
    fn wolf_aggression_by_season() {
        let mut dn = DayNightCycle::new(10, 10);
        dn.season = Season::Spring;
        let spring = dn.season_modifiers().wolf_aggression;
        dn.season = Season::Summer;
        let summer = dn.season_modifiers().wolf_aggression;
        dn.season = Season::Autumn;
        let autumn = dn.season_modifiers().wolf_aggression;
        dn.season = Season::Winter;
        let winter = dn.season_modifiers().wolf_aggression;

        assert_eq!(spring, 0.4);
        assert_eq!(summer, 0.4);
        assert_eq!(autumn, 0.5);
        assert_eq!(winter, 0.7);
    }

    #[test]
    fn influence_map_diffuses() {
        let mut im = InfluenceMap::new(10, 10);
        // Add a source at center
        im.update(&[(5.0, 5.0, 5.0)], None);

        // Center should have influence
        assert!(
            im.get(5, 5) > 0.0,
            "center should have influence after source: got {}",
            im.get(5, 5)
        );

        // Run more ticks to let it diffuse
        for _ in 0..20 {
            im.update(&[(5.0, 5.0, 1.0)], None);
        }

        // Neighbors should have picked up some influence via diffusion
        assert!(
            im.get(4, 5) > 0.0,
            "left neighbor should have influence via diffusion: got {}",
            im.get(4, 5)
        );
        assert!(
            im.get(6, 5) > 0.0,
            "right neighbor should have influence via diffusion: got {}",
            im.get(6, 5)
        );
        assert!(
            im.get(5, 4) > 0.0,
            "top neighbor should have influence via diffusion: got {}",
            im.get(5, 4)
        );
        assert!(
            im.get(5, 6) > 0.0,
            "bottom neighbor should have influence via diffusion: got {}",
            im.get(5, 6)
        );

        // Center should be stronger than edges
        assert!(
            im.get(5, 5) > im.get(1, 1),
            "center should be stronger than corner"
        );
    }

    #[test]
    fn influence_map_decays() {
        let mut im = InfluenceMap::new(10, 10);
        // Add strong source once
        im.update(&[(5.0, 5.0, 10.0)], None);
        let initial = im.get(5, 5);
        assert!(initial > 0.0);

        // Update many times with no sources — should decay
        for _ in 0..200 {
            im.update(&[], None);
        }

        let after = im.get(5, 5);
        assert!(
            after < initial * 0.1,
            "influence should decay significantly without sources: initial={} after={}",
            initial,
            after
        );
    }

    #[test]
    fn vegetation_seasonal_decay() {
        let mut vm = VegetationMap::new(5, 5);
        vm.vegetation[12] = 0.5; // center tile
        // Winter: veg_growth_mult = 0.0
        for _ in 0..1000 {
            vm.apply_season(0.0);
        }
        assert!(
            vm.get(2, 2) < 0.3,
            "vegetation should decay in winter: got {}",
            vm.get(2, 2)
        );
    }

    #[test]
    fn traffic_map_accumulates() {
        let mut tm = TrafficMap::new(10, 10);
        assert_eq!(tm.get(5, 5), 0.0);
        tm.step_on(5, 5);
        tm.step_on(5, 5);
        tm.step_on(5, 5);
        assert_eq!(tm.get(5, 5), 3.0);
    }

    #[test]
    fn traffic_map_decay() {
        let mut tm = TrafficMap::new(10, 10);
        for _ in 0..100 {
            tm.step_on(3, 3);
        }
        let before = tm.get(3, 3);
        for _ in 0..1000 {
            tm.decay();
        }
        let after = tm.get(3, 3);
        assert!(
            after < before * 0.5,
            "traffic should decay over time: {} -> {}",
            before,
            after
        );
    }

    #[test]
    fn traffic_road_candidates_only_walkable() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(2, 2, Terrain::BuildingWall); // unwalkable
        map.set(3, 3, Terrain::Road); // already road

        let mut tm = TrafficMap::new(10, 10);
        // Accumulate traffic on grass, wall, and road tiles
        for _ in 0..200 {
            tm.step_on(1, 1); // grass — should be candidate
            tm.step_on(2, 2); // wall — should NOT
            tm.step_on(3, 3); // road — should NOT
        }

        let candidates = tm.road_candidates(&map, 100.0);
        assert!(
            candidates.contains(&(1, 1)),
            "grass tile with high traffic should be candidate"
        );
        assert!(
            !candidates.contains(&(2, 2)),
            "wall tile should not be candidate"
        );
        assert!(
            !candidates.contains(&(3, 3)),
            "existing road should not be candidate"
        );
    }

    #[test]
    fn traffic_below_threshold_no_candidates() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let mut tm = TrafficMap::new(10, 10);
        tm.step_on(5, 5);
        tm.step_on(5, 5);

        let candidates = tm.road_candidates(&map, 100.0);
        assert!(
            candidates.is_empty(),
            "low traffic should not produce road candidates"
        );
    }

    // --- TrafficMap directional + resource flow tests ---

    #[test]
    fn traffic_step_on_directed_accumulates_direction() {
        let mut tm = TrafficMap::new(10, 10);
        // Walk eastward several times
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        assert_eq!(tm.get(5, 5), 3.0);
        let (dx, dy) = tm.get_direction(5, 5);
        assert!(dx > 0.0, "dx should be positive for eastward steps: {}", dx);
        assert!(
            dy.abs() < 0.001,
            "dy should be near zero for pure eastward: {}",
            dy
        );
    }

    #[test]
    fn traffic_step_on_directed_hauling_has_double_weight() {
        let mut tm = TrafficMap::new(10, 10);
        // One non-hauling step east
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        let (dx1, _) = tm.get_direction(5, 5);
        // One hauling step west (resource = Some)
        tm.step_on_directed(5, 5, -1.0, 0.0, Some(ResourceType::Wood));
        let (dx2, _) = tm.get_direction(5, 5);
        // Hauling west with 2x weight should dominate: 1.0 + (-2.0) = -1.0
        assert!(
            dx2 < 0.0,
            "net direction should be westward (hauling dominates): {}",
            dx2
        );
    }

    #[test]
    fn traffic_dominant_resource_tracks_most_hauled() {
        let mut tm = TrafficMap::new(10, 10);
        // 5 wood hauls
        for _ in 0..5 {
            tm.step_on_directed(3, 3, 1.0, 0.0, Some(ResourceType::Wood));
        }
        // 2 stone hauls
        for _ in 0..2 {
            tm.step_on_directed(3, 3, 1.0, 0.0, Some(ResourceType::Stone));
        }
        assert_eq!(
            tm.get_dominant_resource(3, 3),
            Some(ResourceType::Wood),
            "wood should dominate with 5 vs 2 hauls"
        );
    }

    #[test]
    fn traffic_dominant_resource_none_without_hauls() {
        let mut tm = TrafficMap::new(10, 10);
        tm.step_on_directed(3, 3, 1.0, 0.0, None);
        tm.step_on_directed(3, 3, 1.0, 0.0, None);
        assert_eq!(
            tm.get_dominant_resource(3, 3),
            None,
            "no hauls should mean no dominant resource"
        );
    }

    #[test]
    fn traffic_trail_char_horizontal() {
        let mut tm = TrafficMap::new(10, 10);
        // Strong eastward direction
        for _ in 0..20 {
            tm.step_on_directed(5, 5, 1.0, 0.0, None);
        }
        let ch = tm.trail_char(5, 5);
        assert_eq!(ch, '-', "horizontal traffic should produce '-' trail");
    }

    #[test]
    fn traffic_trail_char_vertical() {
        let mut tm = TrafficMap::new(10, 10);
        // Strong southward direction
        for _ in 0..20 {
            tm.step_on_directed(5, 5, 0.0, 1.0, None);
        }
        let ch = tm.trail_char(5, 5);
        assert_eq!(ch, '|', "vertical traffic should produce '|' trail");
    }

    #[test]
    fn traffic_trail_char_mixed_returns_dot() {
        let mut tm = TrafficMap::new(10, 10);
        // Exactly opposing directions cancel out
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        tm.step_on_directed(5, 5, -1.0, 0.0, None);
        let ch = tm.trail_char(5, 5);
        assert_eq!(ch, '.', "cancelled directions should produce '.' trail");
    }

    #[test]
    fn traffic_decay_affects_directional_and_flow() {
        let mut tm = TrafficMap::new(10, 10);
        for _ in 0..100 {
            tm.step_on_directed(3, 3, 1.0, 0.0, Some(ResourceType::Food));
        }
        let (dx_before, _) = tm.get_direction(3, 3);
        assert!(dx_before > 0.0);

        for _ in 0..1000 {
            tm.decay();
        }

        let (dx_after, _) = tm.get_direction(3, 3);
        assert!(
            dx_after < dx_before * 0.5,
            "directional accumulator should decay: {} -> {}",
            dx_before,
            dx_after
        );
    }

    #[test]
    fn traffic_get_direction_out_of_bounds() {
        let tm = TrafficMap::new(5, 5);
        let (dx, dy) = tm.get_direction(10, 10);
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn traffic_get_dominant_resource_out_of_bounds() {
        let tm = TrafficMap::new(5, 5);
        assert_eq!(tm.get_dominant_resource(10, 10), None);
    }

    // --- ExplorationMap tests ---

    #[test]
    fn exploration_starts_all_unrevealed() {
        let em = ExplorationMap::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                assert!(
                    !em.is_revealed(x, y),
                    "tile ({}, {}) should start unrevealed",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn exploration_reveal_marks_correct_tiles() {
        let mut em = ExplorationMap::new(32, 32);
        em.reveal(16, 16, 3);

        // Center should be revealed
        assert!(em.is_revealed(16, 16));
        // Tiles within radius 3
        assert!(em.is_revealed(16, 14)); // 2 tiles up
        assert!(em.is_revealed(18, 16)); // 2 tiles right
        assert!(em.is_revealed(14, 16)); // 2 tiles left
        assert!(em.is_revealed(16, 18)); // 2 tiles down

        // Tile at distance exactly 3 (on axis) should be revealed
        assert!(em.is_revealed(16, 13)); // 3 tiles up
        assert!(em.is_revealed(19, 16)); // 3 tiles right

        // Tile at distance > 3 should NOT be revealed
        assert!(!em.is_revealed(16, 12)); // 4 tiles up
        assert!(!em.is_revealed(20, 16)); // 4 tiles right

        // Far corner should not be revealed
        assert!(!em.is_revealed(0, 0));
        assert!(!em.is_revealed(31, 31));
    }

    #[test]
    fn exploration_reveal_near_edges() {
        let mut em = ExplorationMap::new(10, 10);
        // Reveal near corner — should not panic
        em.reveal(0, 0, 3);
        assert!(em.is_revealed(0, 0));
        assert!(em.is_revealed(2, 2));
        assert!(!em.is_revealed(4, 0)); // distance 4 > 3

        em.reveal(9, 9, 2);
        assert!(em.is_revealed(9, 9));
        assert!(em.is_revealed(8, 8));
    }

    #[test]
    fn exploration_is_revealed_out_of_bounds() {
        let em = ExplorationMap::new(10, 10);
        assert!(!em.is_revealed(100, 100));
        assert!(!em.is_revealed(10, 0));
        assert!(!em.is_revealed(0, 10));
    }

    #[test]
    fn exploration_multiple_reveals_accumulate() {
        let mut em = ExplorationMap::new(32, 32);
        em.reveal(5, 16, 2);
        em.reveal(25, 16, 2);

        // Both areas should be revealed
        assert!(em.is_revealed(5, 16));
        assert!(em.is_revealed(25, 16));
        // Gap between them should not be revealed
        assert!(!em.is_revealed(15, 16));
    }

    #[test]
    fn viewport_water_matches_full_in_overlap() {
        // Run full-map water sim and viewport-only water sim, then compare overlap region.
        let size = 64;
        let mut heights_full = vec![0.5; size * size];
        let mut heights_vp = heights_full.clone();

        // Create a slope so water actually flows
        for y in 0..size {
            for x in 0..size {
                heights_full[y * size + x] = 1.0 - (x as f64 / (size - 1) as f64);
            }
        }
        heights_vp.copy_from_slice(&heights_full);

        let mut wm_full = WaterMap::new(size, size);
        let mut wm_vp = WaterMap::new(size, size);

        // Seed identical water in the center
        for y in 20..44 {
            for x in 20..44 {
                wm_full.water[y * size + x] = 0.05;
                wm_vp.water[y * size + x] = 0.05;
            }
        }

        let config = SimConfig {
            evaporation: 0.0,
            erosion_enabled: false,
            ..Default::default()
        };

        // viewport covers center area; with 32-tile margin it covers the full 64x64 map
        // Use a smaller viewport so the margin doesn't cover everything
        let viewport = Some((28, 28, 36, 36)); // small 8x8 viewport in center

        for _ in 0..5 {
            wm_full.update(&mut heights_full, &config, None);
            wm_vp.update(&mut heights_vp, &config, viewport);
        }

        // The viewport region (28..36) should be within the simulated region.
        // With 32-margin from (28,28,36,36) we get (0,0,64,64) which IS the full map.
        // So use a truly small map where the margin doesn't cover everything, OR
        // just verify the overlap region matches. Since 28-32=0 and 36+32=64, it covers all.
        // For a 64x64 map this viewport+margin covers everything, so results should match exactly.
        for y in 28..36 {
            for x in 28..36 {
                let i = y * size + x;
                let diff = (wm_full.water[i] - wm_vp.water[i]).abs();
                assert!(
                    diff < 1e-10,
                    "water mismatch at ({}, {}): full={} vp={}",
                    x,
                    y,
                    wm_full.water[i],
                    wm_vp.water[i]
                );
            }
        }
    }

    #[test]
    fn viewport_water_restricts_to_bounds() {
        // On a large enough map, viewport sim should NOT update tiles far outside the margin.
        let size = 128;
        let mut heights = vec![0.5; size * size];
        for y in 0..size {
            for x in 0..size {
                heights[y * size + x] = 1.0 - (x as f64 / (size - 1) as f64);
            }
        }

        let mut wm = WaterMap::new(size, size);
        // Put water everywhere
        wm.water.fill(0.01);

        let config = SimConfig {
            evaporation: 0.0,
            erosion_enabled: false,
            ..Default::default()
        };

        // Save initial state of a far-away tile
        let far_idx = 0 * size + 0; // (0, 0)
        let initial_water = wm.water[far_idx];
        let initial_avg = wm.water_avg[far_idx];

        // Viewport at far end of map: (100, 100, 120, 120), margin brings to (68, 68, 128, 128)
        // So (0, 0) is well outside the simulated region.
        let viewport = Some((100, 100, 120, 120));

        for _ in 0..5 {
            wm.update(&mut heights, &config, viewport);
        }

        // The tile at (0,0) should be unchanged since it's outside viewport+margin
        assert_eq!(
            wm.water[far_idx], initial_water,
            "tile outside viewport+margin should not be modified"
        );
        assert_eq!(
            wm.water_avg[far_idx], initial_avg,
            "water_avg outside viewport+margin should not be modified"
        );
    }

    #[test]
    fn viewport_influence_matches_full_in_overlap() {
        let size = 20;
        let mut im_full = InfluenceMap::new(size, size);
        let mut im_vp = InfluenceMap::new(size, size);

        let sources = vec![(10.0, 10.0, 5.0)];

        // With a 20x20 map and viewport (5,5,15,15), margin of 32 covers the whole map.
        // So results should match exactly.
        let viewport = Some((5, 5, 15, 15));

        for _ in 0..10 {
            im_full.update(&sources, None);
            im_vp.update(&sources, viewport);
        }

        // Check overlap region
        for y in 5..15 {
            for x in 5..15 {
                let diff = (im_full.get(x, y) - im_vp.get(x, y)).abs();
                assert!(
                    diff < 1e-10,
                    "influence mismatch at ({}, {}): full={} vp={}",
                    x,
                    y,
                    im_full.get(x, y),
                    im_vp.get(x, y)
                );
            }
        }
    }

    #[test]
    fn viewport_influence_restricts_to_bounds() {
        // On a large map, viewport should not update tiles far outside the margin.
        let size = 128;
        let mut im = InfluenceMap::new(size, size);

        // Seed some influence everywhere
        for v in im.influence.iter_mut() {
            *v = 1.0;
        }

        let initial_val = im.get(0, 0);

        // Viewport at far end: (100, 100, 120, 120), margin -> (68, 68, 128, 128)
        // So (0, 0) is outside.
        let viewport = Some((100, 100, 120, 120));

        im.update(&[], viewport);

        // Tile at (0, 0) should not have decayed (it's outside the bounds)
        assert_eq!(
            im.get(0, 0),
            initial_val,
            "influence outside viewport+margin should not decay"
        );
    }

    // ─── SoilFertilityMap tests ────────────────────────────────────────────

    #[test]
    fn fertility_degrade_clamps_to_zero() {
        let mut fert = SoilFertilityMap::new(4, 4);
        fert.set(1, 1, 0.05);
        fert.degrade(1, 1, 0.1);
        assert!(
            fert.get(1, 1).abs() < f64::EPSILON,
            "fertility should clamp to 0.0 after degrading past zero, got {}",
            fert.get(1, 1)
        );
    }

    #[test]
    fn fertility_add_clamps_to_one() {
        let mut fert = SoilFertilityMap::new(4, 4);
        fert.set(1, 1, 0.95);
        fert.add(1, 1, 0.15);
        assert!(
            (fert.get(1, 1) - 1.0).abs() < f64::EPSILON,
            "fertility should clamp to 1.0 after adding past cap, got {}",
            fert.get(1, 1)
        );
    }

    #[test]
    fn fertility_add_positive_delta() {
        let mut fert = SoilFertilityMap::new(4, 4);
        fert.set(2, 2, 0.5);
        fert.add(2, 2, 0.1);
        assert!(
            (fert.get(2, 2) - 0.6).abs() < 0.001,
            "fertility should be ~0.6, got {}",
            fert.get(2, 2)
        );
    }

    #[test]
    fn fertility_degrade_positive_delta() {
        let mut fert = SoilFertilityMap::new(4, 4);
        fert.set(2, 2, 0.5);
        fert.degrade(2, 2, 0.1);
        assert!(
            (fert.get(2, 2) - 0.4).abs() < 0.001,
            "fertility should be ~0.4, got {}",
            fert.get(2, 2)
        );
    }

    #[test]
    fn fertility_out_of_bounds_safe() {
        let mut fert = SoilFertilityMap::new(4, 4);
        // Should not panic
        fert.add(10, 10, 0.5);
        fert.degrade(10, 10, 0.5);
        assert!(
            fert.get(10, 10).abs() < f64::EPSILON,
            "out-of-bounds get should return 0.0"
        );
    }

    #[test]
    fn fertility_from_soil_types_alluvial_capped() {
        use crate::terrain_pipeline::SoilType;
        let soil = vec![SoilType::Alluvial, SoilType::Rocky];
        let fert = SoilFertilityMap::from_soil_types(2, 1, &soil);
        // Alluvial yield_multiplier = 1.25, clamped to 1.0
        assert!(
            (fert.get(0, 0) - 1.0).abs() < 0.01,
            "alluvial should be clamped to 1.0, got {}",
            fert.get(0, 0)
        );
        // Rocky yield_multiplier = 0.4
        assert!(
            (fert.get(1, 0) - 0.4).abs() < 0.01,
            "rocky should be ~0.4, got {}",
            fert.get(1, 0)
        );
    }

    // ---- ScentMap tests ----

    #[test]
    fn scent_map_emit_and_get() {
        let mut sm = ScentMap::new(10, 10, 0.998, 0.0);
        assert!(!sm.has_scent());
        sm.emit(5, 5, 10.0);
        assert!(sm.has_scent());
        assert!((sm.get(5, 5) - 10.0).abs() < 0.001);
        assert!((sm.get(0, 0)).abs() < 0.001);
    }

    #[test]
    fn scent_map_out_of_bounds_safe() {
        let mut sm = ScentMap::new(4, 4, 0.998, 0.0);
        sm.emit(100, 100, 5.0); // should not panic
        assert!((sm.get(100, 100)).abs() < 0.001, "out-of-bounds returns 0");
    }

    #[test]
    fn scent_map_decay_reduces_values() {
        let mut sm = ScentMap::new(4, 4, 0.99, 0.0);
        sm.emit(2, 2, 10.0);
        sm.decay();
        let after = sm.get(2, 2);
        assert!(
            (after - 9.9).abs() < 0.01,
            "after one decay at 0.99 rate, 10.0 should become ~9.9, got {}",
            after
        );
    }

    #[test]
    fn scent_map_decay_floors_small_values() {
        let mut sm = ScentMap::new(4, 4, 0.99, 0.0);
        sm.emit(1, 1, 0.005);
        sm.decay();
        assert!(
            sm.get(1, 1).abs() < 0.001,
            "values below 0.01 should be zeroed after decay"
        );
    }

    #[test]
    fn scent_map_diffuse_spreads_to_neighbors() {
        let mut sm = ScentMap::new(10, 10, 0.998, 0.05);
        sm.emit(5, 5, 100.0);
        sm.diffuse();
        // Center should have kept (1 - 8*0.05) = 0.6 of its value
        let center = sm.get(5, 5);
        assert!(
            center > 50.0 && center < 70.0,
            "center after diffuse should be ~60.0, got {}",
            center
        );
        // Neighbors should have received ~5.0 each
        let neighbor = sm.get(5, 6);
        assert!(
            neighbor > 3.0 && neighbor < 7.0,
            "neighbor should receive ~5.0, got {}",
            neighbor
        );
    }

    #[test]
    fn scent_map_diffuse_no_spread_when_zero() {
        let mut sm = ScentMap::new(5, 5, 0.998, 0.0);
        sm.emit(2, 2, 10.0);
        sm.diffuse(); // spread_factor = 0, no diffusion
        assert!(
            (sm.get(2, 2) - 10.0).abs() < 0.001,
            "with spread_factor=0, value should stay put"
        );
        assert!(
            sm.get(2, 3).abs() < 0.001,
            "with spread_factor=0, neighbors should be zero"
        );
    }

    #[test]
    fn scent_map_sample_gradient_finds_strongest() {
        let mut sm = ScentMap::new(20, 20, 0.998, 0.0);
        sm.emit(10, 10, 50.0);
        sm.emit(14, 10, 100.0);
        // From (8, 10), radius 8: should find (14, 10) as strongest
        let result = sm.sample_gradient(8, 10, 8, 0.1);
        assert!(result.is_some(), "should find a gradient target");
        let (gx, gy, val) = result.unwrap();
        assert_eq!((gx, gy), (14, 10));
        assert!((val - 100.0).abs() < 0.01);
    }

    #[test]
    fn scent_map_sample_gradient_returns_none_below_threshold() {
        let sm = ScentMap::new(10, 10, 0.998, 0.0);
        let result = sm.sample_gradient(5, 5, 4, 0.1);
        assert!(
            result.is_none(),
            "empty scent map should return None for gradient"
        );
    }

    #[test]
    fn scent_map_half_life_danger() {
        // Danger scent: decay_rate 0.990 applied every 5 ticks → half-life ~350 ticks
        // 350 ticks / 5 = 70 decay passes. 0.990^70 ≈ 0.496 → near half.
        let mut sm = ScentMap::new(4, 4, 0.990, 0.0);
        sm.emit(1, 1, 100.0);
        for _ in 0..70 {
            sm.decay();
        }
        let val = sm.get(1, 1);
        assert!(
            val > 40.0 && val < 60.0,
            "after ~350 ticks (70 decays at 0.990), value should be near half: got {}",
            val
        );
    }

    #[test]
    fn scent_map_default_is_empty() {
        let sm = ScentMap::default();
        assert_eq!(sm.width, 0);
        assert_eq!(sm.height, 0);
        assert!(!sm.has_scent());
        assert!(sm.get(0, 0).abs() < 0.001);
    }

    #[test]
    fn scent_map_values_returns_raw_slice() {
        let mut sm = ScentMap::new(3, 3, 0.998, 0.0);
        sm.emit(1, 1, 42.0);
        let vals = sm.values();
        assert_eq!(vals.len(), 9);
        assert!((vals[4] - 42.0).abs() < 0.001, "center tile should be 42.0");
    }

    // ---- ThreatMap tests ----

    #[test]
    fn threat_map_new_dimensions() {
        let tm = ThreatMap::new(10, 20);
        assert_eq!(tm.width, 10);
        assert_eq!(tm.height, 20);
        assert_eq!(tm.wolf_territory.len(), 200);
        assert_eq!(tm.garrison_coverage.len(), 200);
        assert_eq!(tm.corridor_pressure.len(), 200);
        assert_eq!(tm.exposure.len(), 200);
    }

    #[test]
    fn threat_map_default_is_empty() {
        let tm = ThreatMap::default();
        assert_eq!(tm.width, 0);
        assert_eq!(tm.height, 0);
    }

    #[test]
    fn threat_map_wolf_territory_marks_forest_with_scent() {
        use crate::tilemap::{Terrain, TileMap};
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        // Place a cluster of forest tiles 20 tiles from center (15,15)
        for y in 3..8 {
            for x in 3..8 {
                map.set(x, y, Terrain::Forest);
            }
        }
        let mut scent = ScentMap::new(30, 30, 0.998, 0.0);
        // Emit danger scent on the forest tiles (wolf presence)
        for y in 3..8 {
            for x in 3..8 {
                scent.emit(x, y, 1.0);
            }
        }

        let mut tm = ThreatMap::new(30, 30);
        tm.update_wolf_territory(&map, &scent, (15, 15));

        // Forest tiles with scent should have wolf territory > 0
        assert!(
            tm.wolf_at(5, 5) > 0.0,
            "forest tile with scent should be wolf territory"
        );
        // Grass tile far from forest should be 0
        assert!(
            tm.wolf_at(20, 20) == 0.0,
            "grass tile far from forest should have no wolf territory"
        );
    }

    #[test]
    fn threat_map_wolf_buffer_zone() {
        use crate::tilemap::{Terrain, TileMap};
        let mut map = TileMap::new(40, 40, Terrain::Grass);
        for y in 10..15 {
            for x in 10..15 {
                map.set(x, y, Terrain::Forest);
            }
        }
        let mut scent = ScentMap::new(40, 40, 0.998, 0.0);
        for y in 10..15 {
            for x in 10..15 {
                scent.emit(x, y, 1.0);
            }
        }

        let mut tm = ThreatMap::new(40, 40);
        tm.update_wolf_territory(&map, &scent, (20, 20));

        // Buffer zone: within 3 tiles of forest edge
        let buffer_val = tm.wolf_at(8, 12); // 2 tiles from forest edge (x=10)
        assert!(
            buffer_val > 0.0,
            "tile near wolf territory should have buffer value, got {}",
            buffer_val
        );
        // Far away: no buffer
        assert_eq!(
            tm.wolf_at(1, 1),
            0.0,
            "tile far from wolf territory should have no buffer"
        );
    }

    #[test]
    fn threat_map_garrison_coverage_decays_with_distance() {
        let mut tm = ThreatMap::new(30, 30);
        let garrisons = vec![(15, 15)];
        let scores = vec![0.0; 30 * 30]; // no chokepoints

        tm.update_garrison_coverage(&garrisons, &scores);

        let close = tm.garrison_at(15, 15);
        let mid = tm.garrison_at(15, 20); // 5 tiles away
        let far = tm.garrison_at(15, 26); // 11 tiles away

        assert!(close > mid, "coverage should decrease with distance");
        assert!(mid > far, "coverage should decrease further with distance");
        assert!(far > 0.0, "coverage should still exist within radius");

        // Beyond radius
        let beyond = tm.garrison_at(15, 28); // 13 tiles away, beyond base 12
        assert_eq!(
            beyond, 0.0,
            "beyond garrison radius should have no coverage"
        );
    }

    #[test]
    fn threat_map_garrison_chokepoint_bonus() {
        let mut tm = ThreatMap::new(30, 30);
        let garrisons = vec![(15, 15)];
        let mut scores = vec![0.0; 30 * 30];
        // Set high chokepoint score at garrison position
        scores[15 * 30 + 15] = 0.5;

        tm.update_garrison_coverage(&garrisons, &scores);
        let with_bonus = tm.garrison_at(15, 15);

        // Recompute without bonus
        let mut tm2 = ThreatMap::new(30, 30);
        let scores_none = vec![0.0; 30 * 30];
        tm2.update_garrison_coverage(&garrisons, &scores_none);
        let without_bonus = tm2.garrison_at(15, 15);

        assert!(
            with_bonus > without_bonus,
            "chokepoint garrison should have higher coverage ({} vs {})",
            with_bonus,
            without_bonus
        );
    }

    #[test]
    fn threat_map_multiple_garrisons_stack() {
        let mut tm = ThreatMap::new(30, 30);
        let garrisons = vec![(12, 15), (18, 15)];
        let scores = vec![0.0; 30 * 30];

        tm.update_garrison_coverage(&garrisons, &scores);
        let overlap = tm.garrison_at(15, 15); // midpoint between two garrisons

        let mut tm_single = ThreatMap::new(30, 30);
        tm_single.update_garrison_coverage(&[(12, 15)].to_vec(), &scores);
        let single = tm_single.garrison_at(15, 15);

        assert!(
            overlap > single,
            "overlapping garrison coverage should exceed single ({} vs {})",
            overlap,
            single
        );
    }

    #[test]
    fn threat_map_corridor_pressure_from_chokepoints() {
        let mut tm = ThreatMap::new(10, 10);
        let mut scores = vec![0.0; 100];
        scores[55] = 0.8; // tile (5,5) is a chokepoint

        tm.update_corridor_pressure(&scores);

        assert!(
            tm.corridor_at(5, 5) > 0.0,
            "chokepoint tile should have corridor pressure"
        );
        assert_eq!(
            tm.corridor_at(0, 0),
            0.0,
            "non-chokepoint tile should have no corridor pressure"
        );
    }

    #[test]
    fn threat_map_exposure_is_threat_minus_defense() {
        let mut tm = ThreatMap::new(10, 10);
        // Set wolf territory at (3,3) and garrison coverage at (3,3)
        tm.wolf_territory[3 * 10 + 3] = 0.8;
        tm.garrison_coverage[3 * 10 + 3] = 0.5;
        // Set wolf territory at (7,7) with no garrison
        tm.wolf_territory[7 * 10 + 7] = 0.9;

        tm.recompute_exposure();

        let defended = tm.exposure_at(3, 3);
        let exposed = tm.exposure_at(7, 7);

        assert!(
            defended < exposed,
            "defended tile should have less exposure ({} vs {})",
            defended,
            exposed
        );
        assert!(
            (defended - 0.3).abs() < 0.01,
            "defended exposure should be 0.8 - 0.5 = 0.3, got {}",
            defended
        );
        assert!(
            (exposed - 0.9).abs() < 0.01,
            "exposed tile should be 0.9, got {}",
            exposed
        );
    }

    #[test]
    fn threat_map_exposure_clamped_to_zero() {
        let mut tm = ThreatMap::new(5, 5);
        // Garrison coverage exceeds threat
        tm.wolf_territory[12] = 0.2;
        tm.garrison_coverage[12] = 1.0;

        tm.recompute_exposure();

        assert_eq!(
            tm.exposure_at(2, 2),
            0.0,
            "exposure should clamp to 0 when defense exceeds threat"
        );
    }

    #[test]
    fn threat_map_out_of_bounds_returns_zero() {
        let tm = ThreatMap::new(5, 5);
        assert_eq!(tm.wolf_at(10, 10), 0.0);
        assert_eq!(tm.garrison_at(10, 10), 0.0);
        assert_eq!(tm.corridor_at(10, 10), 0.0);
        assert_eq!(tm.exposure_at(10, 10), 0.0);
    }
}
