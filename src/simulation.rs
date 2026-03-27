use rand::RngExt;

/// Parallel grid of water depth, layered on top of a height map.
/// Water flows downhill, erodes terrain, and evaporates over time.
pub struct WaterMap {
    pub width: usize,
    pub height: usize,
    water: Vec<f64>,
    water_temp: Vec<f64>,   // transfer buffer for this frame
    water_avg: Vec<f64>,    // smoothed for rendering
}

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

    /// Run one tick of water flow + optional erosion.
    /// `heights` is the terrain height map (same dimensions), and may be modified by erosion.
    pub fn update(&mut self, heights: &mut Vec<f64>, config: &SimConfig) {
        // clear temp buffer
        self.water_temp.fill(0.0);

        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;

                // update smoothed average
                self.water_avg[i] = self.water_avg[i] * config.avg_factor
                    + self.water[i] * (1.0 - config.avg_factor);

                let h = heights[i] + self.water[i];

                // find lowest neighbor (cardinal first, then diagonal with sqrt(2) bias)
                let mut best_x = x as i32;
                let mut best_y = y as i32;
                let mut best_h = h;

                // cardinal directions
                for &(dx, dy) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = self.wrapping_coords(x as i32 + dx, y as i32 + dy);
                    let ni = ny * self.width + nx;
                    let nh = heights[ni] + self.water[ni];
                    if nh < best_h {
                        best_x = x as i32 + dx;
                        best_y = y as i32 + dy;
                        best_h = nh;
                    }
                }

                // diagonal directions — only prefer if drop is > sqrt(2)x the cardinal best
                for &(dx, dy) in &[(1, 1), (-1, -1), (1, -1), (-1, 1)] {
                    let (nx, ny) = self.wrapping_coords(x as i32 + dx, y as i32 + dy);
                    let ni = ny * self.width + nx;
                    let nh = heights[ni] + self.water[ni];
                    if (h - nh) > (h - best_h) * 1.4142 {
                        best_x = x as i32 + dx;
                        best_y = y as i32 + dy;
                        best_h = nh;
                    }
                }

                // flow water downhill
                if best_h < h - 0.0001 {
                    let diff_raw = h - best_h;
                    let mut diff = diff_raw * config.flow_fraction;

                    // dampen based on existing water depth (prevents cutting too deep)
                    diff *= 1.0 - self.water[i];
                    // can't flow more water than we have
                    if diff > self.water[i] {
                        diff = self.water[i];
                    }

                    let (bx, by) = self.wrapping_coords(best_x, best_y);
                    let bi = by * self.width + bx;
                    self.water_temp[bi] += diff;
                    self.water_temp[i] -= diff;
                }
            }
        }

        // apply transfers, erosion, and evaporation
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;

                // erosion: flowing water modifies terrain height
                if config.erosion_enabled {
                    let change = self.water_temp[i];
                    let mut erode = if change > 0.0 {
                        change * 0.5 // deposition is gentler
                    } else {
                        change
                    };

                    // scale erosion by water amount
                    if self.water[i] > 0.001 {
                        erode *= (erode * 0.1 / self.water[i]).abs();
                    } else {
                        erode *= (erode * 40.0).abs();
                    }

                    erode *= config.erosion_strength;

                    // distribute erosion to neighbors (kernel from original)
                    heights[i] += erode / 8.0;
                    for &(dx, dy, w) in &[
                        (1i32, 0i32, 16.0), (-1, 0, 16.0), (0, 1, 16.0), (0, -1, 16.0),
                        (1, 1, 22.63), (-1, -1, 22.63), (1, -1, 22.63), (-1, 1, 22.63),
                    ] {
                        let (nx, ny) = self.wrapping_coords(x as i32 + dx, y as i32 + dy);
                        heights[ny * self.width + nx] += erode / w;
                    }
                }

                // apply water transfer and evaporation
                self.water[i] = (self.water[i] + self.water_temp[i] - config.evaporation)
                    .clamp(0.0, 1.0);
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
}
