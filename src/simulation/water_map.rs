use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::SimConfig;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_heights(w: usize, h: usize, val: f64) -> Vec<f64> {
        vec![val; w * h]
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
}
