//! Canonical simulation state — the single source of truth.
//!
//! All persistent, causal quantities live here. If something affects
//! future evolution, it must be in WorldState. Systems read this +
//! derived data, produce deltas, and those deltas create the next state.
//!
//! See docs/design/cross_cutting/state_driven_architecture.md

use crate::hydrology::HydroMap;
use crate::pipe_water::PipeWater;
use crate::simulation::moisture::MoistureMap;
use crate::simulation::vegetation::VegetationMap;
use crate::simulation::wind::WindField;

/// All persistent simulation state in one place.
/// Systems are the only writers. Derived data is computed from this.
pub struct WorldState {
    pub width: usize,
    pub height: usize,

    // ── Terrain ──
    /// Heightmap — modified by erosion systems.
    pub heights: Vec<f64>,

    // ── Water ──
    /// Surface water depth + 8-directional flux. Single source of truth for water.
    /// Ocean = boundary condition (constant depth at map edges).
    /// Rivers = seeded from discharge field.
    /// Rain/floods = added by weather system.
    pub water: PipeWater,

    // ── Atmosphere ──
    /// Wind velocity field + atmospheric moisture (moisture_carried).
    pub wind: WindField,

    // ── Soil ──
    /// Soil moisture content (0-1 per tile).
    pub moisture: MoistureMap,

    // ── Biology ──
    /// Vegetation density (0-1 per tile).
    pub vegetation: VegetationMap,

    // ── Hydrology ──
    /// Discharge + momentum fields from particle erosion.
    /// Discharge determines WHERE rivers flow.
    /// Momentum enables meandering.
    pub hydro: HydroMap,
}

impl WorldState {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            width: w,
            height: h,
            heights: vec![0.5; w * h],
            water: PipeWater::new(w, h),
            wind: WindField::new(w, h),
            moisture: MoistureMap::new(w, h),
            vegetation: VegetationMap::new(w, h),
            hydro: HydroMap::new(w, h),
        }
    }

    /// Get discharge at a tile (convenience accessor).
    pub fn discharge(&self, x: usize, y: usize) -> f64 {
        let i = y * self.width + x;
        if i < self.hydro.discharge.len() {
            self.hydro.discharge[i]
        } else {
            0.0
        }
    }
}
