use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
