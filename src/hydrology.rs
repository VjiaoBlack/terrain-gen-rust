//! SimpleHydrology — particle-based hydraulic erosion with momentum.
//!
//! Line-by-line translation of Nick McDonald's SimpleHydrology:
//!   Source: ~/Projects/SimpleHydrology/source/ (water.h, world.h, cellpool.h)
//!   Repo:   https://github.com/weigert/SimpleHydrology
//!   Blog:   https://nickmcd.me/2023/12/12/meandering-rivers-in-particle-based-hydraulic-erosion-simulations/
//!
//! This is a FAITHFUL port — each function matches Nick's C++ implementation.
//! DO NOT change parameters or logic without checking against the source.

use std::f64::consts::SQRT_2;

// ─── Per-cell data (cellpool.h cell struct) ─────────────────────────────────

/// Per-cell hydrological state. Matches Nick's `quad::cell` (8 floats).
#[derive(Clone, Debug)]
pub struct HydroMap {
    pub width: usize,
    pub height: usize,
    pub discharge: Vec<f64>,
    pub momentum_x: Vec<f64>,
    pub momentum_y: Vec<f64>,
    discharge_track: Vec<f64>,
    momentum_x_track: Vec<f64>,
    momentum_y_track: Vec<f64>,
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
}

/// Erosion parameters. All values from Nick's source code.
#[derive(Clone, Debug)]
pub struct HydroParams {
    pub evap_rate: f64,
    pub deposition_rate: f64,
    pub min_vol: f64,
    pub max_age: u32,
    pub entrainment: f64,
    pub gravity: f64,
    pub momentum_transfer: f64,
    pub lrate: f64,
    pub max_diff: f64,
    pub settling: f64,
    /// Height below which particles don't spawn (Nick: 0.1).
    /// NOT a termination condition — particles can flow into low areas.
    pub sea_level: f64,
    /// Height amplification for normal calculation (Nick: mapscale=80).
    pub map_scale: f64,
}

impl Default for HydroParams {
    fn default() -> Self {
        Self {
            evap_rate: 0.001,       // water.h:43
            deposition_rate: 0.1,   // water.h:44
            min_vol: 0.01,          // water.h:45
            max_age: 500,           // water.h:46
            entrainment: 10.0,      // water.h:48
            gravity: 1.0,           // water.h:49
            momentum_transfer: 1.0, // water.h:50
            lrate: 0.1,             // world.h:42
            max_diff: 0.01,         // world.h:43
            settling: 0.8,          // world.h:44
            sea_level: 0.1,         // world.h:71 (spawn check)
            map_scale: 80.0,        // cellpool.h:185
        }
    }
}

// ─── Surface normal (cellpool.h _normal) ────────────────────────────────────

/// Compute surface normal using 4 cross products from diagonal quadrants.
/// This matches Nick's `_normal()` in cellpool.h:182-204.
/// The Y component is scaled by map_scale (80) for visible slope.
fn surface_normal(heights: &[f64], w: usize, h: usize, x: usize, y: usize, map_scale: f64) -> (f64, f64, f64) {
    let get_h = |xi: i32, yi: i32| -> f64 {
        if xi < 0 || yi < 0 || xi >= w as i32 || yi >= h as i32 {
            return heights[y * w + x]; // clamp to self
        }
        heights[yi as usize * w + xi as usize]
    };

    let ix = x as i32;
    let iy = y as i32;
    let h0 = heights[y * w + x];
    let s = (1.0, map_scale, 1.0); // Nick's vec3(1.0, mapscale, 1.0)

    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;

    // 4 cross products from diagonal quadrants (Nick's exact code)
    // Quadrant (+x, +y)
    if ix + 1 < w as i32 && iy + 1 < h as i32 {
        let a = (0.0 * s.0, (get_h(ix, iy + 1) - h0) * s.1, 1.0 * s.2);
        let b = (1.0 * s.0, (get_h(ix + 1, iy) - h0) * s.1, 0.0 * s.2);
        // cross(a, b)
        nx += a.1 * b.2 - a.2 * b.1;
        ny += a.2 * b.0 - a.0 * b.2;
        nz += a.0 * b.1 - a.1 * b.0;
    }

    // Quadrant (-x, -y)
    if ix - 1 >= 0 && iy - 1 >= 0 {
        let a = (0.0 * s.0, (get_h(ix, iy - 1) - h0) * s.1, -1.0 * s.2);
        let b = (-1.0 * s.0, (get_h(ix - 1, iy) - h0) * s.1, 0.0 * s.2);
        nx += a.1 * b.2 - a.2 * b.1;
        ny += a.2 * b.0 - a.0 * b.2;
        nz += a.0 * b.1 - a.1 * b.0;
    }

    // Quadrant (+x, -y)
    if ix + 1 < w as i32 && iy - 1 >= 0 {
        let a = (1.0 * s.0, (get_h(ix + 1, iy) - h0) * s.1, 0.0 * s.2);
        let b = (0.0 * s.0, (get_h(ix, iy - 1) - h0) * s.1, -1.0 * s.2);
        nx += a.1 * b.2 - a.2 * b.1;
        ny += a.2 * b.0 - a.0 * b.2;
        nz += a.0 * b.1 - a.1 * b.0;
    }

    // Quadrant (-x, +y)
    if ix - 1 >= 0 && iy + 1 < h as i32 {
        let a = (-1.0 * s.0, (get_h(ix - 1, iy) - h0) * s.1, 0.0 * s.2);
        let b = (0.0 * s.0, (get_h(ix, iy + 1) - h0) * s.1, 1.0 * s.2);
        nx += a.1 * b.2 - a.2 * b.1;
        ny += a.2 * b.0 - a.0 * b.2;
        nz += a.0 * b.1 - a.1 * b.0;
    }

    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len > 0.0 {
        (nx / len, ny / len, nz / len)
    } else {
        (0.0, 1.0, 0.0) // flat
    }
}

// ─── Drop descent (water.h Drop::descend) ───────────────────────────────────

/// Water particle. Matches Nick's `Drop` struct (water.h:12-39).
struct Drop {
    pos: (f64, f64),
    speed: (f64, f64),
    volume: f64,
    sediment: f64,
    age: u32,
}

impl Drop {
    fn new(x: f64, y: f64) -> Self {
        Self { pos: (x, y), speed: (0.0, 0.0), volume: 1.0, sediment: 0.0, age: 0 }
    }

    /// One step of descent. Returns true to continue, false to stop.
    /// Matches Nick's `Drop::descend()` (water.h:58-156) line by line.
    fn descend(
        &mut self,
        heights: &mut [f64],
        hydro: &mut HydroMap,
        params: &HydroParams,
    ) -> bool {
        let w = hydro.width;
        let h = hydro.height;

        let ix = self.pos.0.floor() as i32;
        let iy = self.pos.1.floor() as i32;
        if ix < 0 || iy < 0 || ix >= w as i32 || iy >= h as i32 {
            return false; // OOB
        }
        let ipos = iy as usize * w + ix as usize;

        // Surface normal (cellpool.h _normal — 4 cross products)
        let n = surface_normal(heights, w, h, ix as usize, iy as usize, params.map_scale);

        // Termination checks (water.h:74-82)
        if self.age > params.max_age {
            heights[ipos] += self.sediment;
            return false;
        }
        if self.volume < params.min_vol {
            heights[ipos] += self.sediment;
            return false;
        }

        // Effective deposition rate (water.h:86-87)
        let eff_d = (params.deposition_rate * (1.0 - hydro.root_density[ipos])).max(0.0);

        // Gravity force (water.h:95) — uses n.x and n.z (XZ plane of 3D normal)
        self.speed.0 += params.gravity * n.0 / self.volume;
        self.speed.1 += params.gravity * n.2 / self.volume;

        // Momentum transfer force (water.h:97-99)
        let fx = hydro.momentum_x[ipos];
        let fy = hydro.momentum_y[ipos];
        let flen = (fx * fx + fy * fy).sqrt();
        let slen = (self.speed.0 * self.speed.0 + self.speed.1 * self.speed.1).sqrt();
        if flen > 0.0 && slen > 0.0 {
            let dot = (fx / flen) * (self.speed.0 / slen) + (fy / flen) * (self.speed.1 / slen);
            // Nick: speed += lodsize*momentumTransfer*dot/(volume + cell.discharge)*fspeed
            let factor = params.momentum_transfer * dot / (self.volume + hydro.discharge[ipos]);
            self.speed.0 += factor * fx;
            self.speed.1 += factor * fy;
        }

        // Normalize speed to sqrt(2) step size (water.h:108-109)
        let speed_len = (self.speed.0 * self.speed.0 + self.speed.1 * self.speed.1).sqrt();
        if speed_len > 0.0 {
            self.speed.0 = SQRT_2 * self.speed.0 / speed_len;
            self.speed.1 = SQRT_2 * self.speed.1 / speed_len;
        } else {
            return false; // no force
        }

        // Update position (water.h:111)
        self.pos.0 += self.speed.0;
        self.pos.1 += self.speed.1;

        // Track discharge and momentum at OLD position (water.h:115-117)
        hydro.discharge_track[ipos] += self.volume;
        hydro.momentum_x_track[ipos] += self.volume * self.speed.0;
        hydro.momentum_y_track[ipos] += self.volume * self.speed.1;

        // Height at new position (water.h:120-124)
        // OOB: use current height - 0.002 (slight downhill, not a hard break)
        let h2 = {
            let nx = self.pos.0.floor() as i32;
            let ny = self.pos.1.floor() as i32;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                heights[ipos] - 0.002
            } else {
                heights[ny as usize * w + nx as usize]
            }
        };

        // Sediment equilibrium (water.h:127-132)
        // c_eq uses erf-transformed discharge from node->discharge()
        let discharge_erf = erf_approx(0.4 * hydro.discharge[ipos]);
        let mut c_eq = (1.0 + params.entrainment * discharge_erf) * (heights[ipos] - h2);
        if c_eq < 0.0 { c_eq = 0.0; }
        let c_diff = c_eq - self.sediment;
        self.sediment += eff_d * c_diff;
        heights[ipos] -= eff_d * c_diff;

        // Evaporate — sediment concentrates FIRST, then volume decreases (water.h:135-136)
        self.sediment /= 1.0 - params.evap_rate;
        self.volume *= 1.0 - params.evap_rate;

        // OOB check after move (water.h:139-142)
        {
            let nx = self.pos.0.floor() as i32;
            let ny = self.pos.1.floor() as i32;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                self.volume = 0.0;
                return false;
            }
        }

        // Cascade at NEW position (water.h:151) — NOT old position
        let cx = self.pos.0.floor() as usize;
        let cy = self.pos.1.floor() as usize;
        if cx < w && cy < h {
            cascade(heights, w, h, cx, cy, params);
        }

        self.age += 1;
        true
    }
}

// ─── Cascade (world.h World::cascade) ───────────────────────────────────────

/// Thermal erosion cascade. Matches Nick's `World::cascade()` (world.h:90-168).
/// CRITICAL: neighbors are sorted by height ascending before processing.
fn cascade(
    heights: &mut [f64],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    params: &HydroParams,
) {
    let dirs: [(i32, i32); 8] = [
        (-1, -1), (-1, 0), (-1, 1),
        (0, -1),           (0, 1),
        (1, -1),  (1, 0),  (1, 1),
    ];

    let i = y * w + x;

    // Collect valid neighbors with height and distance
    let mut neighbors: Vec<(usize, f64, f64)> = Vec::with_capacity(8); // (index, height, distance)
    for &(dx, dy) in &dirs {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
            continue;
        }
        let ni = ny as usize * w + nx as usize;
        let dist = ((dx * dx + dy * dy) as f64).sqrt();
        neighbors.push((ni, heights[ni], dist));
    }

    // Sort by height ascending (world.h:129-131) — CRITICAL
    neighbors.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Process sorted neighbors (world.h:133-165)
    for &(ni, nh, dist) in &neighbors {
        let diff = heights[i] - heights[ni];
        if diff == 0.0 { continue; }

        // Excess calculation (world.h:143-148)
        // Below sea level (0.1): no slope limit — excess = full diff
        // Above: excess = |diff| - distance * maxdiff
        let excess = if nh > params.sea_level {
            (diff.abs() - dist * params.max_diff).max(0.0)
        } else {
            diff.abs()
        };

        if excess <= 0.0 { continue; }

        let transfer = params.settling * excess / 2.0;

        // Bidirectional transfer (world.h:157-164)
        if diff > 0.0 {
            heights[i] -= transfer;
            heights[ni] += transfer;
        } else {
            heights[i] += transfer;
            heights[ni] -= transfer;
        }
    }
}

// ─── Erode (world.h World::erode) ──────────────────────────────────────────

/// Approximate error function (good to ~0.001 accuracy).
pub fn erf_approx(x: f64) -> f64 {
    let a = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741
        + t * (-1.453152027 + t * 1.061405429))));
    let result = 1.0 - poly * (-a * a).exp();
    if x >= 0.0 { result } else { -result }
}

/// Run a full erosion cycle. Matches Nick's `World::erode()` (world.h:54-88).
pub fn erode(
    heights: &mut [f64],
    hydro: &mut HydroMap,
    params: &HydroParams,
    cycles: u32,
    seed: u32,
) {
    let w = hydro.width;
    let h = hydro.height;

    // Clear tracking (world.h:56-61)
    for v in &mut hydro.discharge_track { *v = 0.0; }
    for v in &mut hydro.momentum_x_track { *v = 0.0; }
    for v in &mut hydro.momentum_y_track { *v = 0.0; }

    // Spawn and run particles (world.h:64-78)
    let mut rng = seed;
    let next_rand = |state: &mut u32| -> f64 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (*state as f64) / (u32::MAX as f64)
    };

    for _ in 0..cycles {
        let x = next_rand(&mut rng) * (w as f64 - 1.0);
        let y = next_rand(&mut rng) * (h as f64 - 1.0);

        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        let i = iy * w + ix;

        // Only spawn above sea level (world.h:71-72)
        if heights[i] < params.sea_level {
            continue;
        }

        let mut drop = Drop::new(x, y);
        while drop.descend(heights, hydro, params) {}
    }

    // Blend tracking → persistent (world.h:81-86)
    let n = w * h;
    for i in 0..n {
        hydro.discharge[i] += params.lrate * (hydro.discharge_track[i] - hydro.discharge[i]);
        hydro.momentum_x[i] += params.lrate * (hydro.momentum_x_track[i] - hydro.momentum_x[i]);
        hydro.momentum_y[i] += params.lrate * (hydro.momentum_y_track[i] - hydro.momentum_y[i]);
    }
}

/// Run multiple erosion cycles. Returns the HydroMap with discharge/momentum.
pub fn run_hydrology(
    heights: &mut [f64],
    w: usize,
    h: usize,
    params: &HydroParams,
    num_erode_calls: u32,
    particles_per_call: u32,
    seed: u32,
) -> HydroMap {
    let mut hydro = HydroMap::new(w, h);
    for call in 0..num_erode_calls {
        erode(heights, &mut hydro, params, particles_per_call, seed.wrapping_add(call));
    }
    hydro
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp_heights(w: usize, h: usize) -> Vec<f64> {
        let mut heights = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                heights[y * w + x] = 0.1 + (y as f64 / h as f64) * 0.6;
            }
        }
        heights
    }

    #[test]
    fn erode_produces_channels() {
        let w = 64;
        let h = 64;
        let params = HydroParams::default();
        let mut heights = ramp_heights(w, h);
        let original = heights.clone();
        run_hydrology(&mut heights, w, h, &params, 10, 200, 42);
        let eroded = heights.iter().zip(original.iter())
            .filter(|(n, o)| **n < **o - 0.001).count();
        assert!(eroded > 20, "should erode tiles, got {eroded}");
    }

    #[test]
    fn cascade_smooths_spike() {
        let w = 8;
        let h = 8;
        let mut heights = vec![0.5; w * h];
        heights[4 * w + 4] = 0.9;
        let params = HydroParams::default();
        for _ in 0..10 {
            cascade(&mut heights, w, h, 4, 4, &params);
        }
        assert!(heights[4 * w + 4] < 0.7, "spike should be smoothed: {}", heights[4 * w + 4]);
    }

    #[test]
    fn momentum_field_builds_up() {
        let w = 32;
        let h = 32;
        let params = HydroParams::default();
        let mut heights = ramp_heights(w, h);
        let mut hydro = HydroMap::new(w, h);
        for c in 0..10 {
            erode(&mut heights, &mut hydro, &params, 200, 42 + c);
        }
        let total_m: f64 = hydro.momentum_x.iter().zip(hydro.momentum_y.iter())
            .map(|(mx, my)| (mx * mx + my * my).sqrt()).sum();
        assert!(total_m > 0.01, "momentum should build up: {total_m:.6}");
    }

    #[test]
    fn erf_approx_accuracy() {
        assert!((erf_approx(0.0) - 0.0).abs() < 0.001);
        assert!((erf_approx(1.0) - 0.8427).abs() < 0.001);
        assert!((erf_approx(2.0) - 0.9953).abs() < 0.001);
        assert!((erf_approx(-1.0) - (-0.8427)).abs() < 0.001);
    }

    #[test]
    fn particles_dont_spawn_below_sea_level() {
        let w = 32;
        let h = 32;
        let params = HydroParams::default(); // sea_level = 0.1
        let mut heights = vec![0.05; w * h]; // all below sea level
        let original = heights.clone();
        run_hydrology(&mut heights, w, h, &params, 5, 100, 42);
        // Nothing should change — no particles spawned
        assert_eq!(heights, original, "heights should not change below sea level");
    }
}
