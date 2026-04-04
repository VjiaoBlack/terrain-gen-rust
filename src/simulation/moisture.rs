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
