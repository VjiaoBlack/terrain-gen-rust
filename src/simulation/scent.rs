use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
