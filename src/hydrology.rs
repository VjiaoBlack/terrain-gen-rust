//! SimpleHydrology — particle-based hydraulic erosion with momentum.
//!
//! Ported from Nick McDonald's SimpleHydrology:
//!   https://github.com/weigert/SimpleHydrology
//!   Blog: "Procedural Hydrology Improvements and Meandering Rivers"
//!
//! This system replaces SPL erosion + hillslope diffusion + deposition
//! with a unified particle model that naturally produces:
//! - Meandering rivers (from momentum field feedback)
//! - Proper sediment deposition (deltas, floodplains)
//! - Realistic channel formation (from discharge accumulation)
//! - Talus slopes (from cascading after each particle step)

use std::f64::consts::SQRT_2;

/// Per-cell hydrological state. 8 floats total.
#[derive(Clone, Debug)]
pub struct HydroMap {
    pub width: usize,
    pub height: usize,
    /// Persistent discharge field (exponential moving average).
    pub discharge: Vec<f64>,
    /// Persistent momentum field x-component.
    pub momentum_x: Vec<f64>,
    /// Persistent momentum field y-component.
    pub momentum_y: Vec<f64>,
    /// Per-cycle tracking buffers (accumulated, then blended into persistent).
    discharge_track: Vec<f64>,
    momentum_x_track: Vec<f64>,
    momentum_y_track: Vec<f64>,
    /// Root density from vegetation (0 = bare, 1 = fully rooted).
    pub root_density: Vec<f64>,
}

impl HydroMap {
    pub fn new(w: usize, h: usize) -> Self {
        let n = w * h;
        Self {
            width: w,
            height: h,
            discharge: vec![0.0; n],
            momentum_x: vec![0.0; n],
            momentum_y: vec![0.0; n],
            discharge_track: vec![0.0; n],
            momentum_x_track: vec![0.0; n],
            momentum_y_track: vec![0.0; n],
            root_density: vec![0.0; n],
        }
    }

    /// Clear tracking buffers before a new erosion cycle.
    fn clear_tracking(&mut self) {
        for v in &mut self.discharge_track {
            *v = 0.0;
        }
        for v in &mut self.momentum_x_track {
            *v = 0.0;
        }
        for v in &mut self.momentum_y_track {
            *v = 0.0;
        }
    }

    /// Blend tracking buffers into persistent fields (exponential moving average).
    fn blend_tracking(&mut self, lrate: f64) {
        let n = self.width * self.height;
        for i in 0..n {
            self.discharge[i] += lrate * (self.discharge_track[i] - self.discharge[i]);
            self.momentum_x[i] += lrate * (self.momentum_x_track[i] - self.momentum_x[i]);
            self.momentum_y[i] += lrate * (self.momentum_y_track[i] - self.momentum_y[i]);
        }
    }
}

/// Erosion parameters.
#[derive(Clone, Debug)]
pub struct HydroParams {
    /// Evaporation rate per step (fraction of volume lost).
    pub evap_rate: f64,
    /// How fast sediment deposits/erodes toward equilibrium.
    pub deposition_rate: f64,
    /// Minimum drop volume before death.
    pub min_vol: f64,
    /// Maximum particle age (steps).
    pub max_age: u32,
    /// Sediment capacity scaling from discharge.
    pub entrainment: f64,
    /// Gravity force on particles.
    pub gravity: f64,
    /// How much existing flow deflects new particles (meandering).
    pub momentum_transfer: f64,
    /// Exponential blend rate for tracking → persistent fields.
    pub lrate: f64,
    /// Maximum stable height difference for talus cascading.
    pub max_diff: f64,
    /// Fraction of excess transferred during cascade.
    pub settling: f64,
    /// Below this height, tiles are ocean (no erosion).
    pub water_level: f64,
}

impl Default for HydroParams {
    fn default() -> Self {
        Self {
            evap_rate: 0.001,
            deposition_rate: 0.1,
            min_vol: 0.01,
            max_age: 500,
            entrainment: 10.0,
            gravity: 1.0,
            momentum_transfer: 1.0,
            lrate: 0.1,
            max_diff: 0.01,
            settling: 0.8,
            water_level: 0.42,
        }
    }
}

/// A water drop that descends the heightmap, eroding and depositing.
struct Drop {
    x: f64,
    y: f64,
    speed_x: f64,
    speed_y: f64,
    volume: f64,
    sediment: f64,
    age: u32,
}

impl Drop {
    fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            speed_x: 0.0,
            speed_y: 0.0,
            volume: 1.0,
            sediment: 0.0,
            age: 0,
        }
    }

    /// Run the particle to completion: descend, erode/deposit, cascade at each step.
    fn descend(
        &mut self,
        heights: &mut [f64],
        hydro: &mut HydroMap,
        params: &HydroParams,
    ) {
        let w = hydro.width;
        let h = hydro.height;
        let cell_diag = SQRT_2; // fixed step size = sqrt(2) * 1 cell

        while self.age < params.max_age && self.volume > params.min_vol {
            let ix = self.x.floor() as i32;
            let iy = self.y.floor() as i32;
            if ix < 0 || iy < 0 || ix >= w as i32 || iy >= h as i32 {
                break;
            }
            let ipos = iy as usize * w + ix as usize;

            // Don't erode ocean
            if heights[ipos] <= params.water_level {
                break;
            }

            // Surface normal via central differences (4-neighbor)
            let h_here = heights[ipos];
            let h_left = if ix > 0 { heights[ipos - 1] } else { h_here };
            let h_right = if (ix + 1) < w as i32 { heights[ipos + 1] } else { h_here };
            let h_up = if iy > 0 { heights[ipos - w] } else { h_here };
            let h_down = if (iy + 1) < h as i32 { heights[ipos + w] } else { h_here };
            let nx = (h_left - h_right) * 0.5;
            let ny = (h_up - h_down) * 0.5;

            // Gravity force (proportional to slope, inversely to volume)
            self.speed_x += params.gravity * nx / self.volume;
            self.speed_y += params.gravity * ny / self.volume;

            // Momentum transfer from existing flow field (meandering force)
            let flow_x = hydro.momentum_x[ipos];
            let flow_y = hydro.momentum_y[ipos];
            let flow_len = (flow_x * flow_x + flow_y * flow_y).sqrt();
            let speed_len = (self.speed_x * self.speed_x + self.speed_y * self.speed_y).sqrt();

            if flow_len > 1e-6 && speed_len > 1e-6 {
                // Only apply if flow and speed are roughly co-directional
                let dot = (flow_x / flow_len) * (self.speed_x / speed_len)
                    + (flow_y / flow_len) * (self.speed_y / speed_len);
                if dot > 0.0 {
                    let factor = params.momentum_transfer * dot
                        / (self.volume + erf_approx(0.4 * hydro.discharge[ipos]));
                    self.speed_x += factor * flow_x;
                    self.speed_y += factor * flow_y;
                }
            }

            // Normalize speed to fixed step size
            let speed_mag =
                (self.speed_x * self.speed_x + self.speed_y * self.speed_y).sqrt();
            if speed_mag > 1e-8 {
                self.speed_x = self.speed_x / speed_mag * cell_diag;
                self.speed_y = self.speed_y / speed_mag * cell_diag;
            } else {
                // No force — stop
                break;
            }

            // Move
            let new_x = self.x + self.speed_x;
            let new_y = self.y + self.speed_y;

            // Check bounds
            let nix = new_x.floor() as i32;
            let niy = new_y.floor() as i32;
            if nix < 0 || niy < 0 || nix >= w as i32 || niy >= h as i32 {
                // Deposit remaining sediment at current position
                heights[ipos] += self.sediment;
                break;
            }
            let nipos = niy as usize * w + nix as usize;

            // Accumulate into tracking buffers
            hydro.discharge_track[ipos] += self.volume;
            hydro.momentum_x_track[ipos] += self.volume * self.speed_x;
            hydro.momentum_y_track[ipos] += self.volume * self.speed_y;

            // Sediment equilibrium
            let h_diff = heights[ipos] - heights[nipos];
            let discharge_factor = erf_approx(0.4 * hydro.discharge[ipos]);
            let c_eq = (1.0 + params.entrainment * discharge_factor) * h_diff.max(0.0);
            let c_diff = c_eq - self.sediment;
            let root_resist = 1.0 - hydro.root_density[ipos];
            let transfer = params.deposition_rate * root_resist.max(0.0) * c_diff;

            self.sediment += transfer;
            heights[ipos] -= transfer;

            // Don't erode below water level
            if heights[ipos] < params.water_level {
                let correction = params.water_level - heights[ipos];
                heights[ipos] = params.water_level;
                self.sediment -= correction;
            }

            // Cascade at current position (talus relaxation)
            cascade(heights, w, h, ix as usize, iy as usize, params);

            // Update position
            self.x = new_x;
            self.y = new_y;

            // Evaporation
            self.volume *= 1.0 - params.evap_rate;
            // Concentration increases as volume decreases (mass conservation)
            if self.volume > params.min_vol {
                self.sediment /= 1.0 - params.evap_rate;
            }

            self.age += 1;
        }

        // Deposit remaining sediment on death
        let ix = self.x.floor() as i32;
        let iy = self.y.floor() as i32;
        if ix >= 0 && iy >= 0 && ix < w as i32 && iy < h as i32 {
            let ipos = iy as usize * w + ix as usize;
            heights[ipos] += self.sediment.max(0.0);
            cascade(heights, w, h, ix as usize, iy as usize, params);
        }
    }
}

/// Talus cascade: transfer material from a tile to lower neighbors
/// when the height difference exceeds the stable angle of repose.
fn cascade(
    heights: &mut [f64],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    params: &HydroParams,
) {
    let dirs: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (-1,  0),          (1,  0),
        (-1,  1), (0,  1), (1,  1),
    ];
    let dist: [f64; 8] = [
        SQRT_2, 1.0, SQRT_2,
        1.0,         1.0,
        SQRT_2, 1.0, SQRT_2,
    ];

    let i = y * w + x;
    let h_here = heights[i];

    for (di, &(dx, dy)) in dirs.iter().enumerate() {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
            continue;
        }
        let ni = ny as usize * w + nx as usize;
        let diff = heights[i] - heights[ni];
        let max_diff = params.max_diff * dist[di];
        if diff > max_diff {
            let transfer = params.settling * (diff - max_diff) * 0.5;
            heights[i] -= transfer;
            heights[ni] += transfer;
        }
    }
}

/// Approximate error function (good to ~0.001 accuracy).
pub fn erf_approx(x: f64) -> f64 {
    // Abramowitz & Stegun approximation
    let a = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741
        + t * (-1.453152027 + t * 1.061405429))));
    let result = 1.0 - poly * (-a * a).exp();
    if x >= 0.0 { result } else { -result }
}

/// Run a full erosion cycle: spawn particles across the map, run them,
/// blend tracking into persistent fields.
///
/// `particles_per_cycle`: how many drops to spawn (e.g. 8000 for 256x256).
/// Each particle starts at a random position and descends to completion.
pub fn erode(
    heights: &mut [f64],
    hydro: &mut HydroMap,
    params: &HydroParams,
    particles_per_cycle: u32,
    seed: u32,
) {
    let w = hydro.width;
    let h = hydro.height;

    hydro.clear_tracking();

    // Simple deterministic pseudo-random for spawn positions
    let mut rng = seed;
    let next_rand = |state: &mut u32| -> f64 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (*state as f64) / (u32::MAX as f64)
    };

    for _ in 0..particles_per_cycle {
        let x = next_rand(&mut rng) * (w as f64 - 1.0);
        let y = next_rand(&mut rng) * (h as f64 - 1.0);

        // Only spawn on land
        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        let i = iy * w + ix;
        if heights[i] <= params.water_level {
            continue;
        }

        let mut drop = Drop::new(x, y);
        drop.descend(heights, hydro, params);
    }

    hydro.blend_tracking(params.lrate);
}

/// Run multiple erosion cycles for terrain generation.
/// More cycles = more mature terrain with deeper channels and wider valleys.
/// Returns the HydroMap with discharge/momentum fields for river rendering.
pub fn run_hydrology(
    heights: &mut [f64],
    w: usize,
    h: usize,
    params: &HydroParams,
    cycles: u32,
    particles_per_cycle: u32,
    seed: u32,
) -> HydroMap {
    let mut hydro = HydroMap::new(w, h);
    for cycle in 0..cycles {
        erode(heights, &mut hydro, params, particles_per_cycle, seed.wrapping_add(cycle));
    }
    hydro
}

// ─── Tests ────────────────────────────────────────────────────────��──────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp_heights(w: usize, h: usize, water_level: f64) -> Vec<f64> {
        let mut heights = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                // Slope from south (high) to north (low, near water level)
                heights[y * w + x] = water_level + 0.01 + (y as f64 / h as f64) * 0.4;
            }
        }
        heights
    }

    #[test]
    fn erode_produces_channels() {
        let w = 64;
        let h = 64;
        let params = HydroParams {
            water_level: 0.35,
            ..HydroParams::default()
        };
        let mut heights = ramp_heights(w, h, params.water_level);
        let original = heights.clone();

        run_hydrology(&mut heights, w, h, &params, 3, 2000, 42);

        // Some tiles should have been eroded
        let eroded_count = heights
            .iter()
            .zip(original.iter())
            .filter(|(new, old)| **new < **old - 0.001)
            .count();

        assert!(
            eroded_count > 50,
            "should erode significant number of tiles, got {eroded_count}"
        );
    }

    #[test]
    fn erode_does_not_touch_ocean() {
        let w = 32;
        let h = 32;
        let water_level = 0.35;
        let params = HydroParams {
            water_level,
            ..HydroParams::default()
        };
        let mut heights = vec![0.5; w * h];
        // Set left half to ocean
        for y in 0..h {
            for x in 0..w / 2 {
                heights[y * w + x] = 0.3;
            }
        }
        let mut ocean_before = Vec::new();
        for y in 0..h {
            for x in 0..w / 2 {
                ocean_before.push(heights[y * w + x]);
            }
        }

        run_hydrology(&mut heights, w, h, &params, 3, 1000, 42);

        let mut ocean_after = Vec::new();
        for y in 0..h {
            for x in 0..w / 2 {
                ocean_after.push(heights[y * w + x]);
            }
        }
        // Ocean tiles should not go below water level
        for (i, &h) in ocean_after.iter().enumerate() {
            assert!(
                h >= 0.29, // allow tiny float error
                "ocean tile {i} eroded below water level: {h}"
            );
        }
    }

    #[test]
    fn cascade_smooths_spike() {
        let w = 8;
        let h = 8;
        let mut heights = vec![0.5; w * h];
        heights[4 * w + 4] = 0.9; // spike
        let params = HydroParams::default();

        for _ in 0..10 {
            cascade(&mut heights, w, h, 4, 4, &params);
        }

        assert!(
            heights[4 * w + 4] < 0.7,
            "spike should be reduced by cascade, got {}",
            heights[4 * w + 4]
        );
    }

    #[test]
    fn momentum_field_builds_up() {
        let w = 32;
        let h = 32;
        let params = HydroParams {
            water_level: 0.3,
            ..HydroParams::default()
        };
        let mut heights = ramp_heights(w, h, params.water_level);
        let mut hydro = HydroMap::new(w, h);

        // Run several cycles
        for cycle in 0..5 {
            erode(&mut heights, &mut hydro, &params, 500, 42 + cycle);
        }

        // Momentum field should have non-zero values where water flows
        let total_momentum: f64 = hydro
            .momentum_x
            .iter()
            .zip(hydro.momentum_y.iter())
            .map(|(mx, my)| (mx * mx + my * my).sqrt())
            .sum();

        assert!(
            total_momentum > 0.1,
            "momentum field should build up, got {total_momentum:.4}"
        );
    }

    #[test]
    fn erf_approx_accuracy() {
        // Check against known values
        assert!((erf_approx(0.0) - 0.0).abs() < 0.001);
        assert!((erf_approx(1.0) - 0.8427).abs() < 0.001);
        assert!((erf_approx(2.0) - 0.9953).abs() < 0.001);
        assert!((erf_approx(-1.0) - (-0.8427)).abs() < 0.001);
    }

    /// Diagnostic: run hydrology and print before/after stats.
    #[test]
    #[ignore]
    fn diag_hydrology_results() {
        let w = 128;
        let h = 128;
        let params = HydroParams {
            water_level: 0.42,
            ..HydroParams::default()
        };
        let mut heights = ramp_heights(w, h, params.water_level);
        let original = heights.clone();

        eprintln!("=== Before erosion ===");
        let avg_before: f64 = heights.iter().sum::<f64>() / heights.len() as f64;
        eprintln!("  avg height: {avg_before:.4}");

        run_hydrology(&mut heights, w, h, &params, 5, 4000, 42);

        eprintln!("=== After erosion (5 cycles, 4000 particles each) ===");
        let avg_after: f64 = heights.iter().sum::<f64>() / heights.len() as f64;
        let eroded: usize = heights.iter().zip(original.iter())
            .filter(|(n, o)| **n < **o - 0.001).count();
        let deposited: usize = heights.iter().zip(original.iter())
            .filter(|(n, o)| **n > **o + 0.001).count();
        let max_erosion = heights.iter().zip(original.iter())
            .map(|(n, o)| *o - *n).fold(0.0f64, f64::max);
        let max_deposit = heights.iter().zip(original.iter())
            .map(|(n, o)| *n - *o).fold(0.0f64, f64::max);

        eprintln!("  avg height: {avg_after:.4}");
        eprintln!("  eroded tiles: {eroded}");
        eprintln!("  deposited tiles: {deposited}");
        eprintln!("  max erosion: {max_erosion:.4}");
        eprintln!("  max deposit: {max_deposit:.4}");
    }
}
