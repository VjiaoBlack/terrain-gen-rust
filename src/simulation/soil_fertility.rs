use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
