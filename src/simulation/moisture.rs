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

    /// Update moisture from water presence and propagate via wind advection.
    /// Wind pushes moisture along its direction; orographic lift causes
    /// precipitation on windward slopes (rain shadow effect).
    /// Also updates vegetation based on moisture bands.
    pub fn update(
        &mut self,
        pipe_water: &mut crate::pipe_water::PipeWater,
        vegetation: &mut VegetationMap,
        map: &crate::tilemap::TileMap,
        wind: &WindField,
        heights: &[f64],
    ) {
        let w_field = wind.width;
        let h_field = wind.height;
        debug_assert_eq!(w_field, self.width);
        debug_assert_eq!(h_field, self.height);

        // Step 1: moisture from water bodies
        // Oceans and standing water are moisture SOURCES, not sinks.
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let is_ocean = matches!(
                    map.get(x, y),
                    Some(crate::tilemap::Terrain::Water) | Some(crate::tilemap::Terrain::Ice)
                );
                if is_ocean {
                    // Ocean tiles have max moisture (they ARE water)
                    self.moisture[i] = 1.0;
                    continue;
                }
                let w = pipe_water.get_depth(x, y);
                if w > 0.01 {
                    // Standing water: high moisture
                    self.moisture[i] = (self.moisture[i] + 0.1).min(1.0);
                } else if w > 0.0001 {
                    // Trace water: small boost
                    self.moisture[i] = (self.moisture[i] + w * 0.5).min(1.0);
                }
            }
        }

        // Step 2: wind-driven moisture advection + orographic precipitation.
        // Moisture moves in the wind direction. When wind pushes air uphill
        // (dot product of wind direction and terrain slope > 0), moisture
        // precipitates as rain — creating windward/leeward rain shadow.
        const PRECIP_RATE: f64 = 0.4;
        const TRANSPORT_RATE: f64 = 0.2; // fraction of moisture that moves per tick
        let n = self.width * self.height;
        let mut delta = vec![0.0f64; n];
        let mut orographic_rain = vec![0.0f64; n];

        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let m = self.moisture[i];
                if m < 1e-6 {
                    continue;
                }

                // Wind vector at this tile
                let (wx, wy) = wind.get_wind(x, y);
                let speed = wind.get_speed(x, y);

                // Transport amount scales with wind speed
                let transport = m * TRANSPORT_RATE * speed.min(1.0);

                // Compute terrain slope (central differences)
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

                // Orographic lift: wind pushing air uphill
                let orographic_lift = (wx * slope_x + wy * slope_y).max(0.0);
                let precip = transport * orographic_lift * PRECIP_RATE;

                // Remaining moisture after precipitation moves downwind
                let moved = transport - precip;
                orographic_rain[i] += precip;

                if moved > 1e-8 {
                    // Determine target tile from wind direction.
                    // Quantize wind direction into primary + diagonal neighbors.
                    let speed_inv = if speed > 1e-6 { 1.0 / speed } else { 0.0 };
                    let dir_x = wx * speed_inv; // normalized wind direction
                    let dir_y = wy * speed_inv;

                    // Primary target: round wind direction to nearest tile offset
                    let tx = (x as f64 + dir_x).round() as i32;
                    let ty = (y as f64 + dir_y).round() as i32;
                    let ti = self.wrapping_idx(tx, ty);

                    // Also spread to perpendicular neighbor for smoothness
                    let perp_x = -dir_y;
                    let perp_y = dir_x;
                    let lx = (x as f64 + dir_x * 0.5 + perp_x * 0.5).round() as i32;
                    let ly = (y as f64 + dir_y * 0.5 + perp_y * 0.5).round() as i32;
                    let li = self.wrapping_idx(lx, ly);
                    let rx = (x as f64 + dir_x * 0.5 - perp_x * 0.5).round() as i32;
                    let ry = (y as f64 + dir_y * 0.5 - perp_y * 0.5).round() as i32;
                    let ri = self.wrapping_idx(rx, ry);

                    // 60% primary, 20% each perpendicular side
                    delta[ti] += moved * 0.6;
                    delta[li] += moved * 0.2;
                    delta[ri] += moved * 0.2;
                }

                // Subtract what left this cell (transport = precip + moved)
                delta[i] -= transport;
            }
        }

        for i in 0..n {
            self.moisture[i] = (self.moisture[i] + delta[i]).clamp(0.0, 1.0);
        }

        // Feed orographic rain into the pipe water system
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                if orographic_rain[i] > 1e-6 {
                    pipe_water.add_water(x, y, orographic_rain[i] * 0.1);
                }
            }
        }

        // Step 3: box blur
        self.box_blur();

        // Step 3.5: update long-term moisture average
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

    fn box_blur(&mut self) {
        let mut temp = vec![0.0f64; self.width * self.height];
        for y in 0..self.height {
            for x in 0..self.width {
                let mut sum = 0.0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ni = self.wrapping_idx(x as i32 + dx, y as i32 + dy);
                        sum += self.moisture[ni];
                    }
                }
                temp[y * self.width + x] = (sum / 9.0).clamp(0.0, 1.0);
            }
        }
        self.moisture = temp;
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
    fn moisture_rises_near_water() {
        let mut mm = MoistureMap::new(10, 10);
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);
        let wind = default_wind(10, 10);
        let heights = flat_heights(10, 10, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(10, 10);
        pw.add_water(5, 5, 0.5); // water at (5, 5)

        for _ in 0..20 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
        }

        assert!(
            mm.get(5, 5) > 0.05,
            "tile with water should have moisture: got {}",
            mm.get(5, 5)
        );
        // Wind-driven propagation: moisture moves in wind direction (not just +y)
        // Check that moisture spread beyond the source tile
        let total_moisture: f64 = (0..100).map(|i| mm.moisture[i]).sum();
        assert!(
            total_moisture > mm.get(5, 5),
            "moisture should propagate beyond source"
        );
        assert!(
            mm.get(5, 5) > mm.get(0, 0),
            "water tile should be more moist than dry tile"
        );
    }

    #[test]
    fn moisture_decays_without_water() {
        let mut mm = MoistureMap::new(10, 10);
        mm.moisture[55] = 0.8; // some initial moisture, no water
        let mut vm = VegetationMap::new(10, 10);
        let map = grass_map(10, 10);
        let wind = default_wind(10, 10);
        let heights = flat_heights(10, 10, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(10, 10);

        // slower decay (0.95 factor) needs more ticks
        for _ in 0..100 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
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
        let wind = default_wind(10, 10);
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
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
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
        let wind = default_wind(10, 10);
        let heights = flat_heights(10, 10, 0.3);
        let mut pw = crate::pipe_water::PipeWater::new(10, 10);
        *vm.get_mut(5, 5).unwrap() = 0.5; // some initial vegetation

        // slower decay (0.003/tick), 0.5 / 0.003 = ~167 ticks to fully decay
        for _ in 0..200 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
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
        // Place water source at center. After many ticks, moisture should
        // be higher east of the source than west.
        let mut mm = MoistureMap::new(20, 20);
        let mut vm = VegetationMap::new(20, 20);
        let map = grass_map(20, 20);
        let heights = flat_heights(20, 20, 0.3);
        let wind = WindField::compute_from_terrain(&heights, 20, 20, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(20, 20);
        pw.add_water(10, 10, 0.5); // water at (10, 10)

        for _ in 0..50 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
        }

        // Downwind (east of source) should have more moisture than upwind (west)
        let east_moisture = mm.get(15, 10);
        let west_moisture = mm.get(5, 10);
        assert!(
            east_moisture > west_moisture,
            "downwind should have more moisture: east={}, west={}",
            east_moisture,
            west_moisture
        );
    }

    #[test]
    fn orographic_precipitation_rain_shadow() {
        // Wind blows east. Create a ridge in the middle (x=10).
        // Windward side (x<10) should get more moisture deposited;
        // leeward side (x>10) should be drier (rain shadow).
        let w = 20;
        let h = 10;
        let mut heights = vec![0.3; w * h];
        // Create a ridge: heights rise toward x=10, then drop
        for y in 0..h {
            for x in 0..w {
                let dist = (x as f64 - 10.0).abs();
                heights[y * w + x] = 0.3 + (5.0 - dist).max(0.0) * 0.1;
            }
        }

        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        let map = grass_map(w, h);
        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        // Water source on the far west
        for y in 0..h {
            pw.add_water(0, y, 0.3);
        }

        for _ in 0..80 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
        }

        // Windward slope (x=7, before ridge peak) vs leeward (x=13, after ridge)
        let windward_avg: f64 = (0..h).map(|y| mm.get(7, y)).sum::<f64>() / h as f64;
        let leeward_avg: f64 = (0..h).map(|y| mm.get(15, y)).sum::<f64>() / h as f64;
        assert!(
            windward_avg > leeward_avg,
            "windward should be wetter than leeward (rain shadow): windward={}, leeward={}",
            windward_avg,
            leeward_avg
        );
    }

    #[test]
    fn orographic_rain_feeds_pipe_water() {
        // Wind blows east into a slope. Orographic rain should add water
        // to the PipeWater system at windward slope tiles.
        let w = 10;
        let h = 5;
        let mut heights = vec![0.3; w * h];
        // Rising terrain from west to east
        for y in 0..h {
            for x in 0..w {
                heights[y * w + x] = 0.3 + x as f64 * 0.05;
            }
        }

        let mut mm = MoistureMap::new(w, h);
        let mut vm = VegetationMap::new(w, h);
        let map = grass_map(w, h);
        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
        let mut pw = crate::pipe_water::PipeWater::new(w, h);
        // Water source on the west edge
        for y in 0..h {
            pw.add_water(0, y, 0.5);
        }

        let total_before: f64 = (0..w * h).map(|i| pw.get_depth(i % w, i / w)).sum();
        for _ in 0..40 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
        }
        let total_after: f64 = (0..w * h).map(|i| pw.get_depth(i % w, i / w)).sum();

        assert!(
            total_after > total_before,
            "orographic rain should add water to pipe system: before={}, after={}",
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

        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);
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
        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

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
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
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
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);

            // 2. Every 3 ticks: wind.advect_moisture (no manual rain)
            if tick % 3 == 0 {
                let map_ref = &map;
                let (precip, evaporated) = wind.advect_moisture(&heights, &|x, y| {
                    pw.get_depth(x, y) > 0.002 || matches!(map_ref.get(x, y), Some(&Terrain::Water))
                });
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
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);

            if tick % 3 == 0 {
                // Manual rain: inject atmospheric moisture (what 'r' toggle does)
                for v in wind.moisture_carried.iter_mut() {
                    *v = (*v + 0.01).min(1.0);
                }

                let map_ref = &map;
                let (precip, evaporated) = wind.advect_moisture(&heights, &|x, y| {
                    pw.get_depth(x, y) > 0.002 || matches!(map_ref.get(x, y), Some(&Terrain::Water))
                });
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
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);

            if tick % 3 == 0 {
                let map_ref = &map;
                let (precip, evaporated) = wind.advect_moisture(&heights, &|x, y| {
                    pw.get_depth(x, y) > 0.002 || matches!(map_ref.get(x, y), Some(&Terrain::Water))
                });
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
        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

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
        mm.update(&mut pw, &mut vm, &map, &wind, &heights);

        eprintln!("After 1 update (y=5 cross-section):");
        for x in 0..w {
            eprint!("{:.2} ", mm.get(x, 5));
        }
        eprintln!();

        // 10 more updates
        for _ in 0..9 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
        }

        eprintln!("After 10 updates (y=5 cross-section):");
        for x in 0..w {
            eprint!("{:.2} ", mm.get(x, 5));
        }
        eprintln!();

        // 50 more
        for _ in 0..50 {
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
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
        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

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
            mm.update(&mut pw, &mut vm, &map, &wind, &heights);
            let pw_after_mm: f64 = pw.depth.iter().sum();
            total_mm_orog_rain += pw_after_mm - pw_before_mm;

            if tick % 3 == 0 {
                let pw_before_adv: f64 = pw.depth.iter().sum();
                let (precip, evaporated) =
                    wind.advect_moisture(&heights, &|x, y| pw.get_depth(x, y) > 0.002);
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
        // Create a moisture source on the west side, wind blowing east.
        // After many MoistureMap updates, moisture should appear on the east side.
        let w = 40;
        let h = 20;
        let heights = vec![0.2f64; w * h]; // flat terrain
        let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        let tilemap = crate::tilemap::TileMap::new(w, h, crate::tilemap::Terrain::Grass);
        let mut pipe_water = crate::pipe_water::PipeWater::new(w, h);
        let mut vegetation = VegetationMap::new(w, h);

        let mut mm = MoistureMap::new(w, h);

        // Seed moisture on the western edge (x=0..3)
        for y in 0..h {
            for x in 0..3 {
                mm.set(x, y, 0.9);
            }
        }

        // Run many update steps; re-seed source each step
        for _ in 0..60 {
            // Re-apply source each step so it doesn't dry out
            for y in 0..h {
                for x in 0..3 {
                    mm.set(x, y, 0.9);
                }
            }
            mm.update(&mut pipe_water, &mut vegetation, &tilemap, &wind, &heights);
        }

        // Check moisture in the middle band (x = 10..20) where advection should
        // have carried it from the western source
        let mut mid_moisture_sum = 0.0;
        for y in 5..15 {
            for x in 10..20 {
                mid_moisture_sum += mm.get(x, y);
            }
        }
        let east_avg = mid_moisture_sum / (10.0 * 10.0);

        eprintln!("=== Moisture Advection Diagnostic ===");
        eprintln!("Mid-band average moisture (x=10..20): {:.4}", east_avg);

        // Sample a few points
        for &x in &[0, 5, 10, 15, 20, 25, 30, 35] {
            if x < w {
                let m = mm.get(x, 10);
                eprintln!("  moisture at ({}, 10): {:.4}", x, m);
            }
        }

        assert!(
            east_avg > 0.01,
            "Wind should carry moisture downwind, got avg {:.4}",
            east_avg
        );
    }
}
