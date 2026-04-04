use serde::{Deserialize, Serialize};

use super::vegetation::VegetationMap;
use super::wind::WindField;

/// Moisture grid: driven by water presence, propagates downwind, drives vegetation.
#[derive(Serialize, Deserialize)]
pub struct MoistureMap {
    pub width: usize,
    pub height: usize,
    pub moisture: Vec<f64>,
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

    /// Update moisture via unified hydrology Step 3: precipitation writes
    /// directly to soil moisture from wind.moisture_carried.
    ///
    /// The single path: wind.moisture_carried -> self.moisture (direct write).
    /// Orographic lift causes heavier precipitation on windward slopes.
    /// Saturation overflow goes to pipe_water surface depth.
    /// Also updates vegetation based on moisture bands.
    pub fn update(
        &mut self,
        pipe_water: &mut crate::pipe_water::PipeWater,
        vegetation: &mut VegetationMap,
        map: &crate::tilemap::TileMap,
        wind: &mut WindField,
        heights: &[f64],
    ) {
        let w_field = wind.width;
        let h_field = wind.height;
        debug_assert_eq!(w_field, self.width);
        debug_assert_eq!(h_field, self.height);

        // Precipitation constants (unified hydrology design doc Step 3)
        const BACKGROUND_RATE: f64 = 0.002; // light rain everywhere wind has moisture
        const OROGRAPHIC_RATE: f64 = 0.3; // heavy rain on windward slopes
        const SATURATION_THRESHOLD: f64 = 0.8;
        const PASSIVE_DECAY: f64 = 0.995; // un-rained tiles dry out

        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;

                // Ocean tiles are always saturated
                let is_ocean = matches!(
                    map.get(x, y),
                    Some(crate::tilemap::Terrain::Water) | Some(crate::tilemap::Terrain::Ice)
                );
                if is_ocean {
                    self.moisture[i] = 1.0;
                    continue;
                }

                // Passive decay: un-rained tiles slowly dry out
                self.moisture[i] *= PASSIVE_DECAY;

                // Compute terrain gradient (central differences)
                let h_here = heights[i];
                let h_left = if x > 0 { heights[i - 1] } else { h_here };
                let h_right = if x + 1 < self.width {
                    heights[i + 1]
                } else {
                    h_here
                };
                let h_up = if y > 0 {
                    heights[i - self.width]
                } else {
                    h_here
                };
                let h_down = if y + 1 < self.height {
                    heights[i + self.width]
                } else {
                    h_here
                };
                let slope_x = (h_right - h_left) * 0.5;
                let slope_y = (h_down - h_up) * 0.5;

                // Wind direction at this tile
                let (wx, wy) = wind.get_wind(x, y);

                // Orographic lift: dot(wind_direction, terrain_gradient).max(0.0)
                let orographic_lift = (wx * slope_x + wy * slope_y).max(0.0);

                // Total precipitation from atmospheric moisture
                let carried = wind.moisture_carried[i];
                let total_precip = carried * (BACKGROUND_RATE + orographic_lift * OROGRAPHIC_RATE);

                // Write precipitation DIRECTLY to soil moisture
                self.moisture[i] += total_precip;

                // Subtract from atmospheric moisture
                wind.moisture_carried[i] = (carried - total_precip).max(0.0);

                // Saturation overflow: excess soil moisture goes to surface water
                if self.moisture[i] > SATURATION_THRESHOLD {
                    let overflow = self.moisture[i] - SATURATION_THRESHOLD;
                    pipe_water.add_water(x, y, overflow * 0.1);
                    self.moisture[i] = SATURATION_THRESHOLD;
                }
            }
        }

        // Update long-term moisture average
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

    /// Default wind field for moisture tests: gentle southward wind (prevailing_dir=PI/2).
    fn default_wind(w: usize, h: usize) -> WindField {
        WindField::compute_from_terrain(
            &flat_heights(w, h, 0.3),
            w,
            h,
            std::f64::consts::FRAC_PI_2,
            0.6,
            None,
        )
    }

    #[test]
    fn step3_precipitation_direct_to_soil_moisture() {
        // 40x40 map, ocean on west (x=0..5), mountain ridge at x=20, wind blowing east.
        // After 100 ticks of wind advection + precipitation:
        // - Tiles x=6..19 (windward) should have moisture > 0.2
        // - Tiles x=25..35 (rain shadow) should have moisture < half of windward avg
        let w = 40;
        let h = 40;

        // Heights: flat everywhere with a gentle mountain ridge at x=20.
        // Keep terrain uniform so wind field stays eastward (prevailing direction).
        // The mountain is just a gentle bump, not a terrain wall.
        let mut heights = vec![0.4; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x < 5 {
                    // Ocean floor (below water level)
                    heights[i] = 0.35;
                } else {
                    // Gentle mountain ridge centered at x=20, Gaussian-ish profile
                    let dist = (x as f64 - 20.0).abs();
                    let ridge = 0.15 * (-dist * dist / 18.0).exp();
                    heights[i] = 0.4 + ridge;
                }
            }
        }

        // TileMap: ocean tiles on x=0..5
        let mut map = TileMap::new(w, h, Terrain::Grass);
        for y in 0..h {
            for x in 0..5 {
                map.set(x, y, Terrain::Water);
            }
        }

        // Wind blowing east (prevailing_dir=0.0)
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        // PipeWater with ocean mask
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..5 {
                pw.set_ocean_boundary(x, y, 0.07);
                pw.add_water(x, y, 0.07);
            }
        }

        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);

        // Run 1000 ticks: wind advection (every 3 ticks) + moisture update (every tick)
        // Wind needs time to evaporate from ocean, carry moisture inland, and precipitate.
        for tick in 0..1000 {
            if tick % 3 == 0 {
                wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
            }
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        // Measure windward moisture (x=6..19, excluding ocean)
        let mut windward_sum = 0.0;
        let mut windward_count = 0;
        for y in 0..h {
            for x in 6..20 {
                windward_sum += mm.get(x, y);
                windward_count += 1;
            }
        }
        let windward_avg = windward_sum / windward_count as f64;

        // Measure rain shadow moisture (x=25..35)
        let mut shadow_sum = 0.0;
        let mut shadow_count = 0;
        for y in 0..h {
            for x in 25..36 {
                shadow_sum += mm.get(x, y);
                shadow_count += 1;
            }
        }
        let shadow_avg = shadow_sum / shadow_count as f64;

        eprintln!("Windward avg moisture (x=6..19): {:.4}", windward_avg);
        eprintln!("Rain shadow avg moisture (x=25..35): {:.4}", shadow_avg);

        assert!(
            windward_avg > 0.2,
            "Windward tiles should have moisture > 0.2, got {:.4}",
            windward_avg
        );
        assert!(
            shadow_avg < windward_avg * 0.5,
            "Rain shadow should have < half windward moisture: shadow={:.4}, windward={:.4}",
            shadow_avg,
            windward_avg
        );
    }

    #[test]
    fn moisture_rises_near_water() {
        // In the unified hydrology, moisture comes from wind.moisture_carried
        // via precipitation. Ocean tiles (Water) are set to 1.0 directly.
        // Wind carries moisture from ocean and precipitates it on nearby land.
        let w = 20;
        let h = 10;
        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        // Ocean on left (x=0..3), grass elsewhere
        let mut map = TileMap::new(w, h, Terrain::Grass);
        for y in 0..h {
            for x in 0..3 {
                map.set(x, y, Terrain::Water);
            }
        }
        let heights = flat_heights(w, h, 0.3);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..3 {
                pw.set_ocean_boundary(x, y, 0.05);
                pw.add_water(x, y, 0.05);
            }
        }

        for tick in 0..200 {
            if tick % 3 == 0 {
                wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
            }
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        // Ocean tiles should have moisture = 1.0
        assert!(
            mm.get(1, 5) > 0.9,
            "ocean tile should have high moisture: got {}",
            mm.get(1, 5)
        );
        // Near-coast land should have some moisture from precipitation
        let coast_moisture = mm.get(4, 5);
        assert!(
            coast_moisture > 0.01,
            "coastal land should have moisture from precipitation: got {}",
            coast_moisture
        );
        // Far inland should have less moisture than coast
        assert!(
            mm.get(4, 5) > mm.get(15, 5),
            "coast should be wetter than far inland: coast={}, far={}",
            mm.get(4, 5),
            mm.get(15, 5)
        );
    }

    #[test]
    fn moisture_decays_without_water() {
        let mut mm = MoistureMap::new(10, 10);
        mm.moisture[55] = 0.8; // some initial moisture, no water
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);
        let mut wind = default_wind(10, 10);
        let heights = flat_heights(10, 10, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(10, 10);

        // Passive decay at 0.995/tick: 0.8 * 0.995^1000 ≈ 0.005
        for _ in 0..1000 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        assert!(
            mm.get(5, 5) < 0.1,
            "moisture should decay without water source: got {}",
            mm.get(5, 5)
        );
    }

    #[test]
    fn vegetation_grows_with_moisture() {
        let mut mm = MoistureMap::new(10, 10);
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);
        let mut wind = default_wind(10, 10);
        let heights = flat_heights(10, 10, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(10, 10);

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
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        assert!(
            vm.get(5, 5) > 0.0,
            "vegetation should grow with sustained moisture: got {}",
            vm.get(5, 5)
        );
    }

    #[test]
    fn vegetation_decays_without_moisture() {
        let mut mm = MoistureMap::new(10, 10);
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);
        let mut wind = default_wind(10, 10);
        let heights = flat_heights(10, 10, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(10, 10);
        *vm.get_mut(5, 5).unwrap() = 0.5; // some initial vegetation

        // slower decay (0.003/tick), 0.5 / 0.003 = ~167 ticks to fully decay
        for _ in 0..200 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        assert!(
            vm.get(5, 5) < 0.1,
            "vegetation should decay without moisture: got {}",
            vm.get(5, 5)
        );
    }

    #[test]
    fn wind_driven_moisture_follows_wind_direction() {
        // Wind blows east (prevailing_dir=0 means east).
        // Ocean on west edge provides moisture source. After wind advection,
        // moisture should be higher near coast (west) than far inland (east).
        let w = 30;
        let h = 10;
        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        let mut map = TileMap::new(w, h, Terrain::Grass);
        for y in 0..h {
            for x in 0..3 {
                map.set(x, y, Terrain::Water);
            }
        }
        let heights = flat_heights(w, h, 0.3);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..3 {
                pw.set_ocean_boundary(x, y, 0.05);
                pw.add_water(x, y, 0.05);
            }
        }

        for tick in 0..500 {
            if tick % 3 == 0 {
                wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
            }
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        // Near-coast land (x=5) should have more moisture than far east (x=25)
        let near_coast: f64 = (0..h).map(|y| mm.get(5, y)).sum::<f64>() / h as f64;
        let far_east: f64 = (0..h).map(|y| mm.get(25, y)).sum::<f64>() / h as f64;
        assert!(
            near_coast > far_east,
            "near-coast should be wetter than far inland: coast={}, far={}",
            near_coast,
            far_east
        );
    }

    #[test]
    fn orographic_precipitation_rain_shadow() {
        // Wind blows east. Ocean on west, ridge at x=15.
        // Windward side should get more moisture than leeward (rain shadow).
        let w = 30;
        let h = 15;
        let mut heights = vec![0.3; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x < 3 {
                    heights[i] = 0.25; // ocean floor
                } else {
                    // Gentle ridge at x=15
                    let dist = (x as f64 - 15.0).abs();
                    heights[i] = 0.3 + 0.15 * (-dist * dist / 12.0).exp();
                }
            }
        }

        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        let mut map = TileMap::new(w, h, Terrain::Grass);
        for y in 0..h {
            for x in 0..3 {
                map.set(x, y, Terrain::Water);
            }
        }
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..3 {
                pw.set_ocean_boundary(x, y, 0.05);
                pw.add_water(x, y, 0.05);
            }
        }

        for tick in 0..800 {
            if tick % 3 == 0 {
                wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
            }
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        // Windward (x=8..13, before ridge) vs leeward (x=20..25, after ridge)
        let mut windward_sum = 0.0;
        for y in 0..h {
            for x in 8..14 {
                windward_sum += mm.get(x, y);
            }
        }
        let windward_avg = windward_sum / (h * 6) as f64;
        let mut leeward_sum = 0.0;
        for y in 0..h {
            for x in 20..26 {
                leeward_sum += mm.get(x, y);
            }
        }
        let leeward_avg = leeward_sum / (h * 6) as f64;
        assert!(
            windward_avg > leeward_avg,
            "windward should be wetter than leeward (rain shadow): windward={:.4}, leeward={:.4}",
            windward_avg,
            leeward_avg
        );
    }

    #[test]
    fn orographic_rain_feeds_pipe_water() {
        // When soil moisture exceeds saturation threshold (0.8), overflow
        // goes to pipe_water. Seed soil moisture near saturation and add
        // atmospheric moisture with orographic lift (slope) to push over.
        let w = 10;
        let h = 5;
        // Rising terrain so orographic lift is significant
        let mut heights = vec![0.3; w * h];
        for y in 0..h {
            for x in 0..w {
                heights[y * w + x] = 0.3 + x as f64 * 0.04;
            }
        }

        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        let map = grass_map(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);

        // Pre-saturate soil moisture so any precipitation causes overflow
        for v in mm.moisture.iter_mut() {
            *v = 0.79;
        }
        // Seed high atmospheric moisture
        for v in wind.moisture_carried.iter_mut() {
            *v = 0.9;
        }

        let total_before: f64 = (0..w * h).map(|i| pw.get_depth(i % w, i / w)).sum();
        for _ in 0..10 {
            // Keep atmospheric moisture high
            for v in wind.moisture_carried.iter_mut() {
                *v = (*v + 0.1).min(1.0);
            }
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }
        let total_after: f64 = (0..w * h).map(|i| pw.get_depth(i % w, i / w)).sum();

        assert!(
            total_after > total_before,
            "saturation overflow should add water to pipe system: before={}, after={}",
            total_before,
            total_after
        );
    }

    // =========================================================================
    // DIAGNOSTIC TESTS — trace data flow through the water/moisture/wind chain
    // Run with: cargo test --lib diag_ -- --ignored --nocapture
    // =========================================================================

    /// Helper: create a map with an ocean (Water) on the left third, grass elsewhere.
    fn ocean_left_map(w: usize, h: usize) -> TileMap {
        let mut map = TileMap::new(w, h, Terrain::Grass);
        for y in 0..h {
            for x in 0..(w / 3) {
                map.set(x, y, Terrain::Water);
            }
        }
        map
    }

    /// Helper: create heights where ocean area is below water_level, land is above.
    fn ocean_left_heights(w: usize, h: usize) -> Vec<f64> {
        let water_level = 0.42;
        let mut heights = vec![0.5; w * h]; // land above water
        for y in 0..h {
            for x in 0..(w / 3) {
                heights[y * w + x] = water_level - 0.05; // ocean below water level
            }
        }
        heights
    }

    // ----- DIAGNOSTIC 1: Initial state inspection -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_1_initial_state_after_worldgen_analog() {
        // Simulate what Game::new does: ocean on left, grass on right,
        // pipe_water seeded on Water tiles, moisture from pipeline analog.
        let w = 60;
        let h = 40;
        let map = ocean_left_map(w, h);
        let heights = ocean_left_heights(w, h);
        let water_level = 0.42;

        // Count Water tiles
        let mut water_tile_count = 0;
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    water_tile_count += 1;
                }
            }
        }

        // Seed pipe_water like Game::new does
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    let i = y * w + x;
                    let depth = (water_level - heights[i]).max(0.01);
                    pw.add_water(x, y, depth);
                }
            }
        }

        // Check pipe_water depths on ocean tiles
        let mut pw_depths: Vec<f64> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    pw_depths.push(pw.get_depth(x, y));
                }
            }
        }
        let pw_avg = pw_depths.iter().sum::<f64>() / pw_depths.len() as f64;
        let pw_min = pw_depths.iter().cloned().fold(f64::INFINITY, f64::min);
        let pw_max = pw_depths.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Initialize moisture like Game::new (from pipeline — simulate with simple proximity)
        let mut mm = MoistureMap::new(w, h);
        // Mimic pipeline: water tiles get 1.0, nearby land gets some
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    mm.set(x, y, 1.0);
                } else {
                    // Distance to nearest water
                    let ocean_edge = w / 3;
                    let dist = if x >= ocean_edge { x - ocean_edge } else { 0 };
                    let m = (1.0 - dist as f64 * 0.05).max(0.0);
                    mm.set(x, y, m);
                }
            }
        }
        mm.avg_moisture = mm.moisture.clone();

        let m_avg = mm.moisture.iter().sum::<f64>() / mm.moisture.len() as f64;
        let m_min = mm.moisture.iter().cloned().fold(f64::INFINITY, f64::min);
        let m_max = mm
            .moisture
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let m_above_half = mm.moisture.iter().filter(|&&v| v > 0.5).count();

        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let avg_speed: f64 = wind.wind_speed.iter().sum::<f64>() / wind.wind_speed.len() as f64;
        let mc_nonzero = wind.moisture_carried.iter().filter(|&&v| v > 1e-8).count();

        eprintln!("=== DIAGNOSTIC 1: Initial State ===");
        eprintln!("Map size: {}x{}", w, h);
        eprintln!(
            "Water tiles: {} / {} total ({:.1}%)",
            water_tile_count,
            w * h,
            water_tile_count as f64 / (w * h) as f64 * 100.0
        );
        eprintln!(
            "PipeWater on ocean: avg={:.4}, min={:.4}, max={:.4}",
            pw_avg, pw_min, pw_max
        );
        eprintln!(
            "Moisture: avg={:.4}, min={:.4}, max={:.4}",
            m_avg, m_min, m_max
        );
        eprintln!(
            "Moisture tiles > 0.5: {} ({:.1}%)",
            m_above_half,
            m_above_half as f64 / (w * h) as f64 * 100.0
        );
        eprintln!(
            "avg_moisture == moisture (initial): {}",
            mm.avg_moisture == mm.moisture
        );
        eprintln!(
            "Wind: avg_speed={:.4}, moisture_carried nonzero={}",
            avg_speed, mc_nonzero
        );
        eprintln!(
            "FINDING: moisture_carried starts at ZERO everywhere — wind has no moisture to carry initially"
        );
        eprintln!(
            "FINDING: PipeWater seeded only on Terrain::Water tiles with depth={:.4}",
            pw_avg
        );
    }

    // ----- DIAGNOSTIC 2: 100 ticks with NO rain -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_2_no_rain_100_ticks() {
        let w = 60;
        let h = 40;
        let map = ocean_left_map(w, h);
        let heights = ocean_left_heights(w, h);
        let water_level = 0.42;

        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    let i = y * w + x;
                    let depth = (water_level - heights[i]).max(0.01);
                    pw.add_water(x, y, depth);
                }
            }
        }

        let mut mm = MoistureMap::new(w, h);
        // Seed moisture: ocean=1.0, land starts at 0
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    mm.set(x, y, 1.0);
                }
            }
        }
        mm.avg_moisture = mm.moisture.clone();

        let mut vm = VegetationMap::new(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        // Snapshot before
        let moisture_before: Vec<f64> = mm.moisture.clone();
        let avg_before: Vec<f64> = mm.avg_moisture.clone();
        let pw_total_before: f64 = (0..w * h).map(|i| pw.depth[i]).sum();
        let ocean_depth_before: f64 = {
            let mut s = 0.0;
            for y in 0..h {
                for x in 0..w / 3 {
                    s += pw.get_depth(x, y);
                }
            }
            s
        };

        eprintln!("=== DIAGNOSTIC 2: 100 ticks, NO rain ===");

        // Run 100 ticks of just moisture update (what step() does)
        for tick in 0..100 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
            // Also step pipe_water like game does
            pw.step(&heights, 0.1);

            if tick == 0 || tick == 9 || tick == 49 || tick == 99 {
                let m_avg = mm.moisture.iter().sum::<f64>() / mm.moisture.len() as f64;
                let avg_m = mm.avg_moisture.iter().sum::<f64>() / mm.avg_moisture.len() as f64;
                // Sample land tile adjacent to ocean (x=w/3, y=h/2)
                let coast_x = w / 3;
                let coast_y = h / 2;
                let inland_x = w / 3 + 10;
                let far_inland_x = w - 5;
                eprintln!(
                    "  tick {}: moisture_avg={:.4}, avg_moisture_avg={:.4}",
                    tick, m_avg, avg_m
                );
                eprintln!(
                    "    coast({},{})={:.4}  inland({},{})={:.4}  far({},{})={:.4}",
                    coast_x,
                    coast_y,
                    mm.get(coast_x, coast_y),
                    inland_x,
                    coast_y,
                    mm.get(inland_x, coast_y),
                    far_inland_x,
                    coast_y,
                    mm.get(far_inland_x, coast_y)
                );
                let ocean_pw_sum = {
                    let mut s = 0.0;
                    for yy in 0..h {
                        for xx in 0..w / 3 {
                            s += pw.get_depth(xx, yy);
                        }
                    }
                    s
                };
                eprintln!(
                    "    ocean(5,{})={:.4}  pipe_water ocean total={:.4}",
                    coast_y,
                    mm.get(5, coast_y),
                    ocean_pw_sum
                );
            }
        }

        let pw_total_after: f64 = (0..w * h).map(|i| pw.depth[i]).sum();
        let ocean_depth_after: f64 = {
            let mut s = 0.0;
            for y in 0..h {
                for x in 0..w / 3 {
                    s += pw.get_depth(x, y);
                }
            }
            s
        };

        // Did moisture change on land tiles?
        let land_moisture_change: f64 = (0..w * h)
            .filter(|&i| {
                let x = i % w;
                let y = i / w;
                !matches!(map.get(x, y), Some(Terrain::Water))
            })
            .map(|i| (mm.moisture[i] - moisture_before[i]).abs())
            .sum();

        eprintln!("--- Summary ---");
        eprintln!(
            "PipeWater total: before={:.4}, after={:.4}, delta={:.4}",
            pw_total_before,
            pw_total_after,
            pw_total_after - pw_total_before
        );
        eprintln!(
            "Ocean pipe depth: before={:.4}, after={:.4}",
            ocean_depth_before, ocean_depth_after
        );
        eprintln!(
            "Land moisture total change (abs): {:.6}",
            land_moisture_change
        );
        eprintln!(
            "wind.moisture_carried: all still zero? {}",
            wind.moisture_carried.iter().all(|&v| v < 1e-8)
        );
        eprintln!("");
        eprintln!("KEY FINDING: MoistureMap.update() sets ocean tiles to 1.0 (step 1)");
        eprintln!("KEY FINDING: Then step 2 tries wind advection, but...");
        eprintln!("  - transport = m * TRANSPORT_RATE(0.2) * speed.min(1.0)");
        let avg_speed = wind.wind_speed.iter().sum::<f64>() / wind.wind_speed.len() as f64;
        eprintln!("  - avg wind speed = {:.4}", avg_speed);
        eprintln!("  - On flat terrain, orographic_lift = 0 so precip = 0");
        eprintln!("  - Moisture DOES move via delta[], but box_blur then DIFFUSES it");
        eprintln!("KEY FINDING: wind.moisture_carried is NEVER written by MoistureMap.update()");
        eprintln!("  The moisture_carried field is only updated by wind.advect_moisture()");
        eprintln!("  which is called in game step() separately — but NOT here in isolation.");
        eprintln!("FINDING: There are TWO independent moisture transport systems:");
        eprintln!("  1. MoistureMap.update() step 2: direct tile-to-tile advection");
        eprintln!("  2. wind.advect_moisture(): atmospheric moisture_carried advection");
        eprintln!("  These are BOTH running in game, potentially competing/conflicting.");
    }

    // ----- DIAGNOSTIC 3: With wind.advect_moisture (full cycle) -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_3_full_water_cycle_with_advect() {
        let w = 60;
        let h = 40;
        let map = ocean_left_map(w, h);
        let heights = ocean_left_heights(w, h);
        let water_level = 0.42;

        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    let i = y * w + x;
                    let depth = (water_level - heights[i]).max(0.01);
                    pw.add_water(x, y, depth);
                }
            }
        }

        let mut mm = MoistureMap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    mm.set(x, y, 1.0);
                }
            }
        }
        mm.avg_moisture = mm.moisture.clone();

        let mut vm = VegetationMap::new(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        let pw_total_before: f64 = pw.depth.iter().sum();

        eprintln!(
            "=== DIAGNOSTIC 3: Full water cycle (moisture + advect_moisture), NO manual rain ==="
        );

        for tick in 0..100 {
            // Replicate game step() order:
            // 1. moisture.update
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);

            // 2. Every 3 ticks: wind.advect_moisture (no manual rain)
            if tick % 3 == 0 {
                let (precip, evaporated) =
                    wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
                // Apply mass conservation
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        if evaporated[i] > 0.0001 {
                            let depth = pw.get_depth(x, y);
                            let remove = evaporated[i].min(depth * 0.5);
                            pw.add_water(x, y, -remove);
                        }
                        if precip[i] > 0.0001 {
                            pw.add_water(x, y, precip[i] * 0.5);
                        }
                        let excess = (wind.moisture_carried[i] - 0.8).max(0.0);
                        if excess > 0.001 {
                            pw.add_water(x, y, excess * 0.1);
                            wind.moisture_carried[i] -= excess * 0.1;
                        }
                    }
                }
            }

            // 3. pipe_water step
            pw.step(&heights, 0.1);

            if tick == 0 || tick == 9 || tick == 49 || tick == 99 {
                let mc_total: f64 = wind.moisture_carried.iter().sum();
                let mc_max = wind.moisture_carried.iter().cloned().fold(0.0f64, f64::max);
                let mc_nonzero = wind.moisture_carried.iter().filter(|&&v| v > 1e-6).count();
                let coast_x = w / 3;
                let coast_y = h / 2;

                eprintln!(
                    "  tick {}: moisture_carried total={:.4}, max={:.4}, nonzero={}/{}",
                    tick,
                    mc_total,
                    mc_max,
                    mc_nonzero,
                    w * h
                );
                eprintln!(
                    "    tile moisture: coast({},{})={:.4}  inland({},{})={:.4}",
                    coast_x,
                    coast_y,
                    mm.get(coast_x, coast_y),
                    coast_x + 10,
                    coast_y,
                    mm.get(coast_x + 10, coast_y)
                );
                eprintln!(
                    "    avg_moisture: coast={:.4}  inland={:.4}",
                    mm.get_avg(coast_x, coast_y),
                    mm.get_avg(coast_x + 10, coast_y)
                );
                eprintln!(
                    "    pipe_water: coast={:.6}  inland={:.6}",
                    pw.get_depth(coast_x, coast_y),
                    pw.get_depth(coast_x + 10, coast_y)
                );
            }
        }

        let pw_total_after: f64 = pw.depth.iter().sum();
        eprintln!("--- Mass Conservation ---");
        eprintln!(
            "PipeWater total: before={:.4}, after={:.4}, delta={:.4} ({:.2}%)",
            pw_total_before,
            pw_total_after,
            pw_total_after - pw_total_before,
            (pw_total_after - pw_total_before) / pw_total_before * 100.0
        );

        // Check: does moisture_carried ever get significant values over ocean?
        let mc_over_ocean: f64 = {
            let mut s = 0.0;
            for y in 0..h {
                for x in 0..w / 3 {
                    s += wind.get_moisture_carried(x, y);
                }
            }
            s
        };
        let mc_over_land: f64 = {
            let mut s = 0.0;
            for y in 0..h {
                for x in w / 3..w {
                    s += wind.get_moisture_carried(x, y);
                }
            }
            s
        };
        eprintln!("moisture_carried over ocean (sum): {:.4}", mc_over_ocean);
        eprintln!("moisture_carried over land (sum): {:.4}", mc_over_land);
        eprintln!("");
        eprintln!("KEY FINDING: advect_moisture picks up moisture over water via PICKUP_RATE=0.08");
        eprintln!("  but is_water check requires pipe_water.depth > 0.002 OR Terrain::Water");
        eprintln!("  Ocean tiles ARE Terrain::Water, so pickup should work.");
        eprintln!("  But: evaporation removes from pipe_water, so ocean depth may drain!");
    }

    // ----- DIAGNOSTIC 3b: With manual rain toggled -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_3b_with_manual_rain() {
        let w = 60;
        let h = 40;
        let map = ocean_left_map(w, h);
        let heights = ocean_left_heights(w, h);
        let water_level = 0.42;

        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    let i = y * w + x;
                    let depth = (water_level - heights[i]).max(0.01);
                    pw.add_water(x, y, depth);
                }
            }
        }

        let mut mm = MoistureMap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    mm.set(x, y, 1.0);
                }
            }
        }
        mm.avg_moisture = mm.moisture.clone();

        let mut vm = VegetationMap::new(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        let pw_total_before: f64 = pw.depth.iter().sum();

        eprintln!("=== DIAGNOSTIC 3b: Full water cycle WITH manual rain ===");

        for tick in 0..100 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);

            if tick % 3 == 0 {
                // Manual rain: inject atmospheric moisture (what 'r' toggle does)
                for v in wind.moisture_carried.iter_mut() {
                    *v = (*v + 0.01).min(1.0);
                }

                let (precip, evaporated) =
                    wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        if evaporated[i] > 0.0001 {
                            let depth = pw.get_depth(x, y);
                            let remove = evaporated[i].min(depth * 0.5);
                            pw.add_water(x, y, -remove);
                        }
                        if precip[i] > 0.0001 {
                            pw.add_water(x, y, precip[i] * 0.5);
                        }
                        let excess = (wind.moisture_carried[i] - 0.8).max(0.0);
                        if excess > 0.001 {
                            pw.add_water(x, y, excess * 0.1);
                            wind.moisture_carried[i] -= excess * 0.1;
                        }
                    }
                }
            }
            pw.step(&heights, 0.1);

            if tick == 0 || tick == 9 || tick == 49 || tick == 99 {
                let coast_x = w / 3;
                let coast_y = h / 2;
                let precip_sum: f64 = {
                    let mut s = 0.0;
                    for yy in 0..h {
                        for xx in 0..w {
                            if !matches!(map.get(xx, yy), Some(Terrain::Water)) {
                                s += pw.get_depth(xx, yy);
                            }
                        }
                    }
                    s
                };
                eprintln!(
                    "  tick {}: land pipe_water={:.4}  coast_moisture={:.4}  inland_moisture({},{})={:.4}  far({},{})={:.4}",
                    tick,
                    precip_sum,
                    mm.get(coast_x, coast_y),
                    coast_x + 10,
                    coast_y,
                    mm.get(coast_x + 10, coast_y),
                    w - 5,
                    coast_y,
                    mm.get(w - 5, coast_y)
                );
                eprintln!(
                    "    mc total={:.4}  mc_max={:.4}",
                    wind.moisture_carried.iter().sum::<f64>(),
                    wind.moisture_carried.iter().cloned().fold(0.0f64, f64::max)
                );
            }
        }

        let pw_total_after: f64 = pw.depth.iter().sum();
        eprintln!("--- Mass ---");
        eprintln!(
            "PipeWater: before={:.4}, after={:.4}, delta={:.4}",
            pw_total_before,
            pw_total_after,
            pw_total_after - pw_total_before
        );

        // Where does rain land? Check if it's wind-directed
        eprintln!("Rain distribution on land (pipe_water depth):");
        let land_third = w / 3;
        let mid_third = 2 * w / 3;
        let left_land: f64 = {
            let mut s = 0.0;
            for y in 0..h {
                for x in land_third..mid_third {
                    s += pw.get_depth(x, y);
                }
            }
            s
        };
        let right_land: f64 = {
            let mut s = 0.0;
            for y in 0..h {
                for x in mid_third..w {
                    s += pw.get_depth(x, y);
                }
            }
            s
        };
        eprintln!(
            "  Near-coast land ({}-{}): {:.6}",
            land_third, mid_third, left_land
        );
        eprintln!("  Far inland ({}-{}): {:.6}", mid_third, w, right_land);
        eprintln!("FINDING: Manual rain injects moisture_carried uniformly (+0.01 everywhere)");
        eprintln!("  This means rain falls everywhere, not preferentially near water.");
        eprintln!("  Orographic precip only matters on slopes — flat terrain gets nothing.");
    }

    // ----- DIAGNOSTIC 4: Moisture -> Vegetation chain -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_4_moisture_vegetation_chain() {
        let w = 60;
        let h = 40;
        let map = ocean_left_map(w, h);
        let heights = ocean_left_heights(w, h);
        let water_level = 0.42;

        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    let i = y * w + x;
                    let depth = (water_level - heights[i]).max(0.01);
                    pw.add_water(x, y, depth);
                }
            }
        }

        let mut mm = MoistureMap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    mm.set(x, y, 1.0);
                }
            }
        }
        mm.avg_moisture = mm.moisture.clone();

        let mut vm = VegetationMap::new(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        eprintln!("=== DIAGNOSTIC 4: Moisture -> Vegetation chain ===");

        // Run 300 ticks with full water cycle
        for tick in 0..300 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);

            if tick % 3 == 0 {
                let (precip, evaporated) =
                    wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        if evaporated[i] > 0.0001 {
                            let depth = pw.get_depth(x, y);
                            let remove = evaporated[i].min(depth * 0.5);
                            pw.add_water(x, y, -remove);
                        }
                        if precip[i] > 0.0001 {
                            pw.add_water(x, y, precip[i] * 0.5);
                        }
                        let excess = (wind.moisture_carried[i] - 0.8).max(0.0);
                        if excess > 0.001 {
                            pw.add_water(x, y, excess * 0.1);
                            wind.moisture_carried[i] -= excess * 0.1;
                        }
                    }
                }
            }
            pw.step(&heights, 0.1);
        }

        let coast_x = w / 3; // first land tile next to ocean
        let coast_y = h / 2;
        let inland_x = coast_x + 10;
        let far_x = w - 5;

        eprintln!("After 300 ticks:");
        eprintln!(
            "  Coast ({},{}): moisture={:.4}, avg_moisture={:.4}, vegetation={:.4}",
            coast_x,
            coast_y,
            mm.get(coast_x, coast_y),
            mm.get_avg(coast_x, coast_y),
            vm.get(coast_x, coast_y)
        );
        eprintln!(
            "  Inland ({},{}): moisture={:.4}, avg_moisture={:.4}, vegetation={:.4}",
            inland_x,
            coast_y,
            mm.get(inland_x, coast_y),
            mm.get_avg(inland_x, coast_y),
            vm.get(inland_x, coast_y)
        );
        eprintln!(
            "  Far ({},{}): moisture={:.4}, avg_moisture={:.4}, vegetation={:.4}",
            far_x,
            coast_y,
            mm.get(far_x, coast_y),
            mm.get_avg(far_x, coast_y),
            vm.get(far_x, coast_y)
        );

        // Check: does avg_moisture reflect coast proximity?
        let coast_avg_m: f64 = (0..h).map(|y| mm.get_avg(coast_x, y)).sum::<f64>() / h as f64;
        let inland_avg_m: f64 = (0..h).map(|y| mm.get_avg(inland_x, y)).sum::<f64>() / h as f64;
        let far_avg_m: f64 = (0..h).map(|y| mm.get_avg(far_x, y)).sum::<f64>() / h as f64;

        eprintln!("");
        eprintln!("Column averages (avg_moisture):");
        eprintln!("  coast x={}: {:.4}", coast_x, coast_avg_m);
        eprintln!("  inland x={}: {:.4}", inland_x, inland_avg_m);
        eprintln!("  far x={}: {:.4}", far_x, far_avg_m);

        // Check vegetation gradient
        let coast_veg: f64 = (0..h).map(|y| vm.get(coast_x, y)).sum::<f64>() / h as f64;
        let inland_veg: f64 = (0..h).map(|y| vm.get(inland_x, y)).sum::<f64>() / h as f64;
        let far_veg: f64 = (0..h).map(|y| vm.get(far_x, y)).sum::<f64>() / h as f64;

        eprintln!("Column averages (vegetation):");
        eprintln!("  coast x={}: {:.4}", coast_x, coast_veg);
        eprintln!("  inland x={}: {:.4}", inland_x, inland_veg);
        eprintln!("  far x={}: {:.4}", far_x, far_veg);

        // Vegetation threshold analysis
        eprintln!("");
        eprintln!("Vegetation growth requires avg_moisture > 0.1");
        eprintln!(
            "  Coast avg_moisture {:.4} {} threshold",
            coast_avg_m,
            if coast_avg_m > 0.1 { "ABOVE" } else { "BELOW" }
        );
        eprintln!(
            "  Inland avg_moisture {:.4} {} threshold",
            inland_avg_m,
            if inland_avg_m > 0.1 { "ABOVE" } else { "BELOW" }
        );
        eprintln!(
            "  Far avg_moisture {:.4} {} threshold",
            far_avg_m,
            if far_avg_m > 0.1 { "ABOVE" } else { "BELOW" }
        );

        eprintln!("");
        eprintln!("CRITICAL CHECK: Is coast greener than far inland?");
        eprintln!(
            "  coast_veg ({:.4}) > far_veg ({:.4}): {}",
            coast_veg,
            far_veg,
            coast_veg > far_veg
        );
    }

    // ----- DIAGNOSTIC 5: Trace the box_blur dilution effect -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_5_box_blur_dilution() {
        // Ocean moisture (1.0) next to dry land (0.0).
        // After one update, how much does box_blur spread vs dilute?
        let w = 20;
        let h = 10;
        let mut map = TileMap::new(w, h, Terrain::Grass);
        for y in 0..h {
            for x in 0..5 {
                map.set(x, y, Terrain::Water);
            }
        }
        let heights = flat_heights(w, h, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        let mut mm = MoistureMap::new(w, h);
        // Ocean tiles at 1.0
        for y in 0..h {
            for x in 0..5 {
                mm.set(x, y, 1.0);
            }
        }

        eprintln!("=== DIAGNOSTIC 5: Box blur dilution effect ===");
        eprintln!("Before update (y=5 cross-section):");
        for x in 0..w {
            eprint!("{:.2} ", mm.get(x, 5));
        }
        eprintln!();

        // Single update
        mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);

        eprintln!("After 1 update (y=5 cross-section):");
        for x in 0..w {
            eprint!("{:.2} ", mm.get(x, 5));
        }
        eprintln!();

        // 10 more updates
        for _ in 0..9 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }

        eprintln!("After 10 updates (y=5 cross-section):");
        for x in 0..w {
            eprint!("{:.2} ", mm.get(x, 5));
        }
        eprintln!();

        // 50 more
        for _ in 0..50 {
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        }
        eprintln!("After 60 updates (y=5 cross-section):");
        for x in 0..w {
            eprint!("{:.2} ", mm.get(x, 5));
        }
        eprintln!();

        eprintln!("");
        eprintln!(
            "FINDING: Box blur averages 3x3 neighborhood. Ocean tiles reset to 1.0 each tick."
        );
        eprintln!(
            "  Land tile at x=5 (adjacent to ocean x=4): gets ~3/9=0.33 from ocean neighbors."
        );
        eprintln!("  Wind advection adds some more, but box_blur dominates the spreading.");
        eprintln!("  The 1/9 averaging means moisture falls off VERY fast from ocean edge.");
        eprintln!("  After just 2-3 tiles, moisture is negligible.");
    }

    // ----- DIAGNOSTIC 6: Wind speed and direction verification -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_6_wind_field_properties() {
        let w = 60;
        let h = 40;
        let heights = ocean_left_heights(w, h);
        // Wind blowing east (prevailing_dir=0.0)
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        eprintln!("=== DIAGNOSTIC 6: Wind field properties ===");
        let avg_speed = wind.wind_speed.iter().sum::<f64>() / (w * h) as f64;
        let max_speed = wind.wind_speed.iter().cloned().fold(0.0f64, f64::max);
        let min_speed = wind
            .wind_speed
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);

        eprintln!(
            "Wind stats: avg={:.4}, min={:.4}, max={:.4}",
            avg_speed, min_speed, max_speed
        );
        eprintln!(
            "Prevailing: dir={:.4} rad, strength={:.4}",
            wind.prevailing_dir, wind.prevailing_strength
        );

        // Sample wind vectors across the map
        let y = h / 2;
        eprintln!("Wind vectors at y={} (mid row):", y);
        for &x in &[0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55] {
            if x < w {
                let (wx, wy) = wind.get_wind(x, y);
                let spd = wind.get_speed(x, y);
                eprintln!("  x={:2}: wx={:+.3} wy={:+.3} speed={:.3}", x, wx, wy, spd);
            }
        }

        // Check: how far does transport_rate move moisture per tick?
        eprintln!("");
        eprintln!("MoistureMap transport analysis:");
        eprintln!("  TRANSPORT_RATE = 0.2");
        eprintln!("  transport = moisture * 0.2 * speed.min(1.0)");
        eprintln!(
            "  At speed={:.3}, moisture=1.0: transport = {:.4}",
            avg_speed,
            0.2 * avg_speed.min(1.0)
        );
        eprintln!("  dir_x = wx/speed (normalized). Round to nearest int = target tile.");
        eprintln!("  So moisture moves at most 1 tile per tick, carrying 20% * speed of source.");
        eprintln!("  But then box_blur averages it with 8 neighbors, diluting by ~1/9.");
        eprintln!("  Net effect: moisture barely moves 1-2 tiles from source.");
        eprintln!("");
        eprintln!("Wind advect_moisture analysis:");
        eprintln!("  TRANSPORT_SPEED = 5.0 (in semi-Lagrangian advection)");
        eprintln!(
            "  At speed={:.3}: effective displacement = {:.1} tiles per call",
            avg_speed,
            avg_speed * 5.0
        );
        eprintln!(
            "  Called every 3 ticks. So moisture_carried moves ~{:.1} tiles/tick.",
            avg_speed * 5.0 / 3.0
        );
        eprintln!("  PICKUP_RATE = 0.08 per call over water.");
        eprintln!("  This is the FAST transport. But it feeds pipe_water, not tile moisture.");
        eprintln!("  The tile moisture (what vegetation reads) comes from MoistureMap.update(),");
        eprintln!("  which has the SLOW 1-tile-per-tick advection + box_blur.");
    }

    // ----- DIAGNOSTIC 7: The two moisture systems interaction -----

    #[test]
    #[ignore] // diagnostic — run manually with: cargo test --lib diag_ -- --ignored --nocapture
    fn diag_7_dual_moisture_system_conflict() {
        // MoistureMap.update() step 2 moves moisture via wind advection on the tile grid.
        // wind.advect_moisture() moves moisture_carried via semi-Lagrangian.
        // MoistureMap.update() also feeds orographic rain into pipe_water.
        // wind.advect_moisture() also feeds orographic rain into pipe_water.
        // This means DOUBLE orographic precipitation!
        //
        // Also: MoistureMap.update() reads pipe_water.depth to boost moisture (step 1),
        // but wind.advect_moisture() adds precipitation to pipe_water.
        // So the chain is: pipe_water -> MoistureMap moisture -> wind advection -> pipe_water (loop!)

        let w = 40;
        let h = 20;
        // Create a slope: terrain rises from west to east
        let mut heights = vec![0.3f64; w * h];
        for y in 0..h {
            for x in 0..w {
                heights[y * w + x] = 0.3 + x as f64 * 0.01;
            }
        }

        let map = grass_map(w, h);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        // Put water on west edge
        for y in 0..h {
            pw.add_water(0, y, 0.5);
            pw.add_water(1, y, 0.5);
        }

        let mut mm = MoistureMap::new(w, h);
        for y in 0..h {
            mm.set(0, y, 1.0);
            mm.set(1, y, 1.0);
        }
        mm.avg_moisture = mm.moisture.clone();

        let mut vm = VegetationMap::new(w, h);
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        eprintln!("=== DIAGNOSTIC 7: Dual moisture system on sloped terrain ===");
        let pw_total_before: f64 = pw.depth.iter().sum();

        // Track orographic rain from BOTH systems
        let mut total_mm_orog_rain = 0.0f64;
        let mut total_wind_precip = 0.0f64;

        for tick in 0..100 {
            // Count pipe_water before moisture update
            let pw_before_mm: f64 = pw.depth.iter().sum();
            mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
            let pw_after_mm: f64 = pw.depth.iter().sum();
            total_mm_orog_rain += pw_after_mm - pw_before_mm;

            if tick % 3 == 0 {
                let pw_before_adv: f64 = pw.depth.iter().sum();
                let (precip, evaporated) =
                    wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        if evaporated[i] > 0.0001 {
                            let depth = pw.get_depth(x, y);
                            let remove = evaporated[i].min(depth * 0.5);
                            pw.add_water(x, y, -remove);
                        }
                        if precip[i] > 0.0001 {
                            pw.add_water(x, y, precip[i] * 0.5);
                        }
                        let excess = (wind.moisture_carried[i] - 0.8).max(0.0);
                        if excess > 0.001 {
                            pw.add_water(x, y, excess * 0.1);
                            wind.moisture_carried[i] -= excess * 0.1;
                        }
                    }
                }
                let pw_after_adv: f64 = pw.depth.iter().sum();
                total_wind_precip += pw_after_adv - pw_before_adv;
            }

            pw.step(&heights, 0.1);
        }

        let pw_total_after: f64 = pw.depth.iter().sum();
        eprintln!(
            "PipeWater: before={:.4}, after={:.4}",
            pw_total_before, pw_total_after
        );
        eprintln!(
            "Orographic rain from MoistureMap.update(): {:.6} total over 100 ticks",
            total_mm_orog_rain
        );
        eprintln!(
            "Net water from wind.advect_moisture(): {:.6} total over 33 calls",
            total_wind_precip
        );
        eprintln!("");
        eprintln!("FINDING: MoistureMap.update() adds orographic rain to pipe_water (line ~210)");
        eprintln!("  AND wind.advect_moisture() adds orographic precip to pipe_water (game step).");
        eprintln!("  This is DOUBLE counting orographic precipitation on slopes.");

        // Cross-section of moisture and vegetation
        let y = h / 2;
        eprintln!("");
        eprintln!("Cross-section at y={} after 100 ticks:", y);
        eprintln!("  x:     moisture  avg_moist  vegetation  pipe_depth");
        for &x in &[0, 2, 5, 10, 15, 20, 25, 30, 35, 39] {
            if x < w {
                eprintln!(
                    "  {:2}:    {:.4}     {:.4}     {:.4}       {:.6}",
                    x,
                    mm.get(x, y),
                    mm.get_avg(x, y),
                    vm.get(x, y),
                    pw.get_depth(x, y)
                );
            }
        }
    }

    #[test]
    fn moisture_advection_carries_across_map() {
        // Ocean on west side, wind blowing east. After wind advection +
        // precipitation, moisture should appear across the map.
        let w = 40;
        let h = 20;
        let heights = vec![0.2f64; w * h]; // flat terrain
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        let mut tilemap = crate::tilemap::TileMap::new(w, h, crate::tilemap::Terrain::Grass);
        for y in 0..h {
            for x in 0..3 {
                tilemap.set(x, y, crate::tilemap::Terrain::Water);
            }
        }
        let mut pipe_water = crate::pipe_water::PipeWater::new(w, h);
        for y in 0..h {
            for x in 0..3 {
                pipe_water.set_ocean_boundary(x, y, 0.05);
                pipe_water.add_water(x, y, 0.05);
            }
        }
        let mut vegetation = VegetationMap::new(w, h);
        let mut mm = MoistureMap::new(w, h);

        // Run wind advection + moisture update
        for tick in 0..600 {
            if tick % 3 == 0 {
                wind.advect_moisture(&heights, &pipe_water.ocean_mask, &mm.moisture);
            }
            mm.update(
                &mut pipe_water,
                &mut vegetation,
                &tilemap,
                &mut wind,
                &heights,
            );
        }

        // Check moisture in the middle band (x = 10..20) where wind-carried
        // precipitation should have deposited moisture
        let mut mid_moisture_sum = 0.0;
        for y in 5..15 {
            for x in 10..20 {
                mid_moisture_sum += mm.get(x, y);
            }
        }
        let east_avg = mid_moisture_sum / (10.0 * 10.0);

        assert!(
            east_avg > 0.01,
            "Wind should carry moisture downwind, got avg {:.4}",
            east_avg
        );
    }
}
