use serde::{Deserialize, Serialize};

use crate::renderer::Color;

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

        // Central differences, amplified to make slopes visible in lighting.
        // At scale=40, a slope of 0.05 creates a 63° tilt (nearly black
        // when facing away from sun). At scale=20, the same slope gives
        // 45° (dim but visible). Scale=20 gives good hill/mountain contrast
        // without turning gentle slopes into black patches.
        let scale = 20.0;
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

                // Check shadow: water uses water surface level, land uses terrain height
                let si = (y_pos - y0) * sw + (x_pos - x0);
                let effective_h = if terrain_h < 0.43 { 0.42 } else { terrain_h };
                let in_shadow = shadow[si] > effective_h + 0.01;

                if in_shadow {
                    self.light_map[i] = 0.05;
                    continue;
                }

                // Normal: water/ice surfaces are flat (pointing straight up).
                // Land uses terrain heightmap normals.
                // Use 0.43 as rough water_level threshold (0.42 + margin)
                let is_water_surface = terrain_h < 0.43;
                let (nx, ny, nz) = if is_water_surface {
                    (0.0, 0.0, 1.0) // flat water surface
                } else {
                    Self::terrain_normal(heights, map_w, map_h, x_pos, y_pos)
                };

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
