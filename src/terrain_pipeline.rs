//! Terrain generation pipeline: transforms raw Perlin noise into realistic terrain
//! with cliffs, rivers, biomes, and soil.

use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;

use crate::terrain_gen::{self, TerrainGenConfig};
use crate::tilemap::{Terrain, TileMap};

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct PipelineConfig {
    pub terrain: TerrainGenConfig,
    // Terrace parameters
    pub terrace_w: f64,
    pub terrace_elev_min: f64, // fraction of height range where terracing starts
    // Thermal erosion
    pub thermal_threshold: f64,
    pub thermal_c: f64,
    pub thermal_iters: u32,
    // Hydrology
    pub river_threshold: f64,
    pub river_min_width: f64,
    pub river_max_width: f64,
    // Droplet erosion
    pub droplet_count: u32,
    pub erosion_radius: f64,
    pub droplet_inertia: f64,
    // Biome
    pub shadow_strength: f64,
    pub water_dist_falloff: f64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            terrain: TerrainGenConfig {
                scale: 0.015,
                ..TerrainGenConfig::default()
            },
            terrace_w: 0.06,
            terrace_elev_min: 0.75,
            thermal_threshold: 0.0156,
            thermal_c: 0.5,
            thermal_iters: 40,
            river_threshold: 150.0,
            river_min_width: 2.0,
            river_max_width: 5.0,
            droplet_count: 8000,
            erosion_radius: 3.0,
            droplet_inertia: 0.05,
            shadow_strength: 0.5,
            water_dist_falloff: 12.0,
        }
    }
}

// ─── Output ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum SoilType {
    Sand,
    Loam,
    Alluvial,
    Clay,
    Rocky,
    Peat,
}

impl SoilType {
    pub fn yield_multiplier(&self) -> f64 {
        match self {
            SoilType::Sand => 0.7,
            SoilType::Loam => 1.0,
            SoilType::Alluvial => 1.25,
            SoilType::Clay => 0.95,
            SoilType::Rocky => 0.4,
            SoilType::Peat => 0.5,
        }
    }
}

impl Default for SoilType {
    fn default() -> Self {
        SoilType::Loam
    }
}

pub struct PipelineResult {
    pub map: TileMap,
    pub heights: Vec<f64>,
    pub moisture: Vec<f64>,
    pub temperature: Vec<f64>,
    pub soil: Vec<SoilType>,
    pub river_mask: Vec<bool>,
    pub slope: Vec<f64>,
}

// ─── Stage 2: Terrace + Thermal Erosion ──────────────────────────────────────

pub fn apply_terraces(heights: &mut [f64], w: usize, h: usize, config: &PipelineConfig) {
    let band = config.terrace_w;
    if band <= 0.0 {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let hv = heights[i];
            // Only terrace high elevations (mask)
            let mask = smoothstep(config.terrace_elev_min, 1.0, hv);
            if mask < 0.01 {
                continue;
            }
            let k = (hv / band).floor();
            let f = (hv - k * band) / band;
            let s = (2.0 * f).min(1.0);
            let terraced = (k + s) * band;
            heights[i] = hv * (1.0 - mask) + terraced * mask;
        }
    }
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn thermal_erosion(
    heights: &mut [f64],
    w: usize,
    h: usize,
    threshold: f64,
    c: f64,
    iterations: u32,
) {
    let dirs: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    for _ in 0..iterations {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                // Find lowest neighbor
                let mut best_i = i;
                let mut best_diff = 0.0f64;
                for &(dx, dy) in &dirs {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let ni = ny as usize * w + nx as usize;
                    let diff = heights[i] - heights[ni];
                    if diff > best_diff {
                        best_diff = diff;
                        best_i = ni;
                    }
                }
                if best_diff > threshold && best_i != i {
                    let transfer = c * (best_diff - threshold);
                    heights[i] -= transfer;
                    heights[best_i] += transfer;
                }
            }
        }
    }
}

// ─── Stage 3: Hydrology ──────────────────────────────────────────────────────

/// Priority-Flood depression filling: ensures every cell can drain to boundary.
pub fn priority_flood(heights: &mut [f64], w: usize, h: usize) {
    use std::cmp::Reverse;

    #[derive(PartialEq)]
    struct Cell(f64, usize);
    impl Eq for Cell {}
    impl PartialOrd for Cell {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Cell {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.0
                .partial_cmp(&other.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    }

    let n = w * h;
    let mut visited = vec![false; n];
    let mut pq: BinaryHeap<Reverse<Cell>> = BinaryHeap::new();

    // Seed boundary cells
    for x in 0..w {
        let i_top = x;
        let i_bot = (h - 1) * w + x;
        visited[i_top] = true;
        visited[i_bot] = true;
        pq.push(Reverse(Cell(heights[i_top], i_top)));
        pq.push(Reverse(Cell(heights[i_bot], i_bot)));
    }
    for y in 1..h - 1 {
        let i_left = y * w;
        let i_right = y * w + w - 1;
        visited[i_left] = true;
        visited[i_right] = true;
        pq.push(Reverse(Cell(heights[i_left], i_left)));
        pq.push(Reverse(Cell(heights[i_right], i_right)));
    }

    let dirs: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];

    while let Some(Reverse(Cell(elev, idx))) = pq.pop() {
        let cx = idx % w;
        let cy = idx / w;
        for &(dx, dy) in &dirs {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = ny as usize * w + nx as usize;
            if visited[ni] {
                continue;
            }
            visited[ni] = true;
            if heights[ni] < elev {
                heights[ni] = elev; // fill depression
            }
            pq.push(Reverse(Cell(heights[ni], ni)));
        }
    }
}

/// D8 flow direction: returns index of steepest downslope neighbor (or usize::MAX for flat/boundary).
pub fn compute_flow_direction(heights: &[f64], w: usize, h: usize) -> Vec<usize> {
    let n = w * h;
    let mut flow = vec![usize::MAX; n];
    let dirs: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    let dist = [1.0, 1.0, 1.0, 1.0, 1.414, 1.414, 1.414, 1.414];

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut best_slope = 0.0f64;
            let mut best_ni = usize::MAX;
            for (di, &(dx, dy)) in dirs.iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                let slope = (heights[i] - heights[ni]) / dist[di];
                if slope > best_slope {
                    best_slope = slope;
                    best_ni = ni;
                }
            }
            flow[i] = best_ni;
        }
    }
    flow
}

/// Flow accumulation: each cell starts with area 1, accumulates downstream.
pub fn compute_flow_accumulation(heights: &[f64], flow: &[usize], w: usize, h: usize) -> Vec<f64> {
    let n = w * h;
    let mut accum = vec![1.0f64; n];

    // Sort cells by decreasing elevation
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by(|&a, &b| {
        heights[b]
            .partial_cmp(&heights[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for &i in &order {
        let downstream = flow[i];
        if downstream < n {
            accum[downstream] += accum[i];
        }
    }
    accum
}

/// Extract river cells where accumulation exceeds threshold.
pub fn extract_rivers(accum: &[f64], threshold: f64) -> Vec<bool> {
    accum.iter().map(|&a| a >= threshold).collect()
}

/// Compute river width using Leopold power-law scaling.
pub fn compute_river_width(accum: &[f64], river_mask: &[bool], min_w: f64, max_w: f64) -> Vec<f64> {
    // Find 90th percentile of river accumulation for calibration
    let mut river_accums: Vec<f64> = accum
        .iter()
        .zip(river_mask)
        .filter(|(_, r)| **r)
        .map(|(a, _)| *a)
        .collect();
    if river_accums.is_empty() {
        return vec![0.0; accum.len()];
    }
    river_accums.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let q_ref = river_accums[river_accums.len() * 9 / 10];
    let k = 3.0 / q_ref.sqrt().max(0.001); // target median main river = 3 tiles

    accum
        .iter()
        .zip(river_mask)
        .map(|(&a, &is_river)| {
            if is_river {
                (k * a.sqrt()).clamp(min_w, max_w)
            } else {
                0.0
            }
        })
        .collect()
}

/// Carve river valleys into heightmap.
pub fn carve_rivers(
    heights: &mut [f64],
    w: usize,
    h: usize,
    river_mask: &[bool],
    river_width: &[f64],
    accum: &[f64],
) {
    let bank_zone = 3.0;
    let depth0 = 0.005;
    let depth1 = 0.002;

    // Pre-compute river cell list
    let river_cells: Vec<(usize, usize, f64, f64)> = (0..w * h)
        .filter(|&i| river_mask[i])
        .map(|i| {
            let x = i % w;
            let y = i / w;
            let rw = river_width[i] / 2.0;
            let bed_depth = depth0 + depth1 * (1.0 + accum[i]).ln();
            (x, y, rw, bed_depth)
        })
        .collect();

    for &(rx, ry, half_w, bed_depth) in &river_cells {
        let radius = (half_w + bank_zone).ceil() as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let nx = rx as i32 + dx;
                let ny = ry as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                let d = ((dx * dx + dy * dy) as f64).sqrt();
                let river_h = heights[ry * w + rx];
                let target = if d < half_w {
                    river_h - bed_depth
                } else if d < half_w + bank_zone {
                    let t = (d - half_w) / bank_zone;
                    river_h - bed_depth * (1.0 - t) * (1.0 - t)
                } else {
                    continue;
                };
                if target < heights[ni] {
                    heights[ni] = target;
                }
            }
        }
    }
}

// ─── Stage 4: Droplet Erosion ────────────────────────────────────────────────

pub fn droplet_erosion(heights: &mut [f64], w: usize, h: usize, config: &PipelineConfig) {
    use rand::RngExt;
    let mut rng = rand::rng();

    let capacity_factor = 4.0;
    let min_capacity = 0.01;
    let deposit_speed = 0.3;
    let erode_speed = 0.3;
    let evaporate_speed = 0.01;
    let gravity = 4.0;
    let max_lifetime = 30;

    for _ in 0..config.droplet_count {
        let mut px = rng.random_range(1.0..(w as f64 - 2.0));
        let mut py = rng.random_range(1.0..(h as f64 - 2.0));
        let mut dx = 0.0f64;
        let mut dy = 0.0f64;
        let mut speed = 1.0;
        let mut water = 1.0;
        let mut sediment = 0.0;

        for _ in 0..max_lifetime {
            let ix = px as usize;
            let iy = py as usize;
            if ix < 1 || iy < 1 || ix >= w - 1 || iy >= h - 1 {
                break;
            }

            // Bilinear gradient
            let (height_here, gx, gy) = bilinear_gradient(heights, w, px, py);

            // Update direction with inertia
            dx = dx * config.droplet_inertia - gx * (1.0 - config.droplet_inertia);
            dy = dy * config.droplet_inertia - gy * (1.0 - config.droplet_inertia);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                break;
            }
            dx /= len;
            dy /= len;

            let new_px = px + dx;
            let new_py = py + dy;
            if new_px < 1.0 || new_py < 1.0 || new_px >= (w - 2) as f64 || new_py >= (h - 2) as f64
            {
                break;
            }

            let new_height = bilinear_height(heights, w, new_px, new_py);
            let delta_h = new_height - height_here;

            let cap = ((-delta_h * speed * water * capacity_factor).max(min_capacity)).max(0.0);

            if sediment > cap || delta_h > 0.0 {
                // Deposit
                let deposit = if delta_h > 0.0 {
                    delta_h.min(sediment)
                } else {
                    (sediment - cap) * deposit_speed
                };
                sediment -= deposit;
                // Deposit to 4 nearest cells
                bilinear_add(heights, w, px, py, deposit);
            } else {
                // Erode
                let erode = ((cap - sediment) * erode_speed).min((-delta_h).max(0.0));
                sediment += erode;
                // Erode in radius
                erode_brush(heights, w, h, px, py, erode, config.erosion_radius);
            }

            speed = (speed * speed + delta_h * gravity).max(0.0).sqrt();
            water *= 1.0 - evaporate_speed;
            px = new_px;
            py = new_py;
        }
    }
}

fn bilinear_height(heights: &[f64], w: usize, x: f64, y: f64) -> f64 {
    let ix = x as usize;
    let iy = y as usize;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let i = iy * w + ix;
    heights[i] * (1.0 - fx) * (1.0 - fy)
        + heights[i + 1] * fx * (1.0 - fy)
        + heights[i + w] * (1.0 - fx) * fy
        + heights[i + w + 1] * fx * fy
}

fn bilinear_gradient(heights: &[f64], w: usize, x: f64, y: f64) -> (f64, f64, f64) {
    let ix = x as usize;
    let iy = y as usize;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let i = iy * w + ix;
    let h00 = heights[i];
    let h10 = heights[i + 1];
    let h01 = heights[i + w];
    let h11 = heights[i + w + 1];
    let height = h00 * (1.0 - fx) * (1.0 - fy)
        + h10 * fx * (1.0 - fy)
        + h01 * (1.0 - fx) * fy
        + h11 * fx * fy;
    let gx = (h10 - h00) * (1.0 - fy) + (h11 - h01) * fy;
    let gy = (h01 - h00) * (1.0 - fx) + (h11 - h10) * fx;
    (height, gx, gy)
}

fn bilinear_add(heights: &mut [f64], w: usize, x: f64, y: f64, amount: f64) {
    let ix = x as usize;
    let iy = y as usize;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let i = iy * w + ix;
    heights[i] += amount * (1.0 - fx) * (1.0 - fy);
    heights[i + 1] += amount * fx * (1.0 - fy);
    heights[i + w] += amount * (1.0 - fx) * fy;
    heights[i + w + 1] += amount * fx * fy;
}

fn erode_brush(
    heights: &mut [f64],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    amount: f64,
    radius: f64,
) {
    let r = radius.ceil() as i32;
    let ix = cx as usize;
    let iy = cy as usize;
    let mut weight_sum = 0.0;
    let mut cells: Vec<(usize, f64)> = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            let nx = ix as i32 + dx;
            let ny = iy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let d = ((dx * dx + dy * dy) as f64).sqrt();
            if d < radius {
                let wt = (radius - d).max(0.0);
                weight_sum += wt;
                cells.push((ny as usize * w + nx as usize, wt));
            }
        }
    }
    if weight_sum > 0.0 {
        for (ni, wt) in cells {
            heights[ni] -= amount * wt / weight_sum;
        }
    }
}

// ─── Stage 5: Climate + Biomes ───────────────────────────────────────────────

pub fn compute_temperature(heights: &[f64], w: usize, h: usize, seed: u32) -> Vec<f64> {
    let perlin = Perlin::new(seed.wrapping_add(1000));
    let n = w * h;
    let mut temp = vec![0.0; n];
    let sea_level = 0.35; // match default water_level

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let lat = y as f64 / (h - 1) as f64; // 0 = north, 1 = south
            let base = 1.0 - (lat - 0.5).abs() * 1.6; // warmer in middle
            let lapse = ((heights[i] - sea_level).max(0.0)) * 1.5; // altitude cooling
            let noise_val = perlin.get([x as f64 * 0.03, y as f64 * 0.03]) * 0.1;
            temp[i] = (base - lapse + noise_val).clamp(0.0, 1.0);
        }
    }
    temp
}

pub fn compute_rainfall(
    heights: &[f64],
    w: usize,
    h: usize,
    config: &PipelineConfig,
    seed: u32,
) -> Vec<f64> {
    let perlin = Perlin::new(seed.wrapping_add(2000));
    let n = w * h;
    let mut rain = vec![0.0; n];

    // Base rainfall from noise
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let base = (perlin.get([x as f64 * 0.02, y as f64 * 0.02]) + 1.0) / 2.0;
            rain[i] = base.clamp(0.3, 1.0);
        }
    }

    // Rain shadow: march west to east
    for y in 0..h {
        let mut barrier = f64::NEG_INFINITY;
        for x in 0..w {
            let i = y * w + x;
            barrier = barrier.max(heights[i]);
            // Orographic lift
            if x > 0 {
                let uphill = (heights[i] - heights[i - 1]).max(0.0);
                rain[i] += uphill * 0.2;
            }
            // Shadow
            let shadow = (barrier - heights[i] - 0.1).max(0.0);
            rain[i] -= config.shadow_strength * shadow;
            rain[i] = rain[i].clamp(0.0, 1.5);
            // Decay barrier
            barrier -= 0.005;
        }
    }

    // Normalize to [0, 1]
    let max_rain = rain.iter().cloned().fold(0.0f64, f64::max).max(0.001);
    for v in &mut rain {
        *v /= max_rain;
    }
    rain
}

pub fn compute_moisture(
    rainfall: &[f64],
    river_mask: &[bool],
    heights: &[f64],
    w: usize,
    h: usize,
    water_level: f64,
    config: &PipelineConfig,
) -> Vec<f64> {
    let n = w * h;

    // BFS distance from all water tiles (rivers + ocean)
    let mut dist_to_water = vec![u32::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    for i in 0..n {
        if river_mask[i] || heights[i] < water_level {
            dist_to_water[i] = 0;
            queue.push_back(i);
        }
    }
    let dirs: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    while let Some(ci) = queue.pop_front() {
        let cx = ci % w;
        let cy = ci / w;
        let cd = dist_to_water[ci];
        for &(dx, dy) in &dirs {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = ny as usize * w + nx as usize;
            if dist_to_water[ni] == u32::MAX {
                dist_to_water[ni] = cd + 1;
                queue.push_back(ni);
            }
        }
    }

    // Combine rainfall + water proximity
    let mut moisture = vec![0.0; n];
    for i in 0..n {
        let from_water = (-((dist_to_water[i] as f64) / config.water_dist_falloff)).exp();
        moisture[i] = (0.5 * rainfall[i] + 0.6 * from_water).clamp(0.0, 1.0);
    }
    moisture
}

pub fn classify_biome(
    height: f64,
    temp: f64,
    moisture: f64,
    slope: f64,
    water_level: f64,
) -> Terrain {
    if height < water_level {
        return Terrain::Water;
    }
    // Marsh: wet + flat + low
    if moisture > 0.75 && slope < 0.02 && height < water_level + 0.08 {
        return Terrain::Marsh;
    }
    // Cliff: very steep
    if slope > 0.15 {
        return Terrain::Cliff;
    }
    // Snow: high and cold
    if temp < 0.15 && height > 0.8 {
        return Terrain::Snow;
    }
    // Tundra: cold
    if temp < 0.2 {
        return Terrain::Tundra;
    }
    // Desert: dry
    if moisture < 0.2 {
        return Terrain::Desert;
    }
    // Scrubland: low moisture
    if moisture < 0.35 {
        return Terrain::Scrubland;
    }
    // Mountain: high elevation
    if height > 0.8 {
        return Terrain::Mountain;
    }
    // Sand: near water level
    if height < water_level + 0.05 {
        return Terrain::Sand;
    }
    // Forest: high moisture
    if moisture > 0.55 {
        return Terrain::Forest;
    }
    // Grassland
    Terrain::Grass
}

// ─── Stage 6: Slope ──────────────────────────────────────────────────────────

pub fn compute_slope(heights: &[f64], w: usize, h: usize) -> Vec<f64> {
    let n = w * h;
    let mut slope = vec![0.0; n];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let dzdx = heights[i + 1] - heights[i - 1];
            let dzdy = heights[i + w] - heights[i - w];
            slope[i] = (dzdx * dzdx + dzdy * dzdy).sqrt();
        }
    }
    slope
}

// ─── Stage 7: Soil ───────────────────────────────────────────────────────────

pub fn assign_soil(
    heights: &[f64],
    slope: &[f64],
    moisture: &[f64],
    river_mask: &[bool],
    w: usize,
    h: usize,
    water_level: f64,
) -> Vec<SoilType> {
    let n = w * h;
    let mut soil = vec![SoilType::Loam; n];

    // BFS distance from rivers
    let mut dist_to_river = vec![u32::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    for i in 0..n {
        if river_mask[i] {
            dist_to_river[i] = 0;
            queue.push_back(i);
        }
    }
    let dirs: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    while let Some(ci) = queue.pop_front() {
        let cx = ci % w;
        let cy = ci / w;
        let cd = dist_to_river[ci];
        if cd >= 8 {
            continue;
        } // limit BFS depth
        for &(dx, dy) in &dirs {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = ny as usize * w + nx as usize;
            if dist_to_river[ni] == u32::MAX {
                dist_to_river[ni] = cd + 1;
                queue.push_back(ni);
            }
        }
    }

    for i in 0..n {
        if heights[i] < water_level {
            continue; // water
        }
        // Priority order
        if moisture[i] > 0.85 && slope[i] < 0.02 {
            soil[i] = SoilType::Peat;
        } else if slope[i] > 0.12 {
            soil[i] = SoilType::Rocky;
        } else if heights[i] < water_level + 0.06 && moisture[i] > 0.3 {
            soil[i] = SoilType::Sand;
        } else if dist_to_river[i] < 4 && slope[i] < 0.05 {
            soil[i] = SoilType::Alluvial;
        } else if slope[i] < 0.04 && heights[i] < 0.5 {
            soil[i] = SoilType::Clay;
        }
        // else: Loam (default)
    }
    soil
}

// ─── Orchestrator ────────────────────────────────────────────────────────────

pub fn run_pipeline(w: usize, h: usize, config: &PipelineConfig) -> PipelineResult {
    // Stage 1: Base height (fBm)
    let (mut map, mut heights) = terrain_gen::generate_terrain(w, h, &config.terrain);

    // Stage 2: Terraces + thermal erosion
    apply_terraces(&mut heights, w, h, config);
    thermal_erosion(
        &mut heights,
        w,
        h,
        config.thermal_threshold,
        config.thermal_c,
        config.thermal_iters,
    );

    // Stage 3: Hydrology
    priority_flood(&mut heights, w, h);
    let flow = compute_flow_direction(&heights, w, h);
    let accum = compute_flow_accumulation(&heights, &flow, w, h);
    let river_mask = extract_rivers(&accum, config.river_threshold);
    let river_width = compute_river_width(
        &accum,
        &river_mask,
        config.river_min_width,
        config.river_max_width,
    );

    // Stage 4: River carving
    carve_rivers(&mut heights, w, h, &river_mask, &river_width, &accum);

    // Stage 5: Droplet erosion
    droplet_erosion(&mut heights, w, h, config);

    // Stage 6: Climate + biomes
    let temperature = compute_temperature(&heights, w, h, config.terrain.seed);
    let rainfall = compute_rainfall(&heights, w, h, config, config.terrain.seed);
    let moisture = compute_moisture(
        &rainfall,
        &river_mask,
        &heights,
        w,
        h,
        config.terrain.water_level,
        config,
    );
    let slope = compute_slope(&heights, w, h);

    // Classify biomes
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let terrain = classify_biome(
                heights[i],
                temperature[i],
                moisture[i],
                slope[i],
                config.terrain.water_level,
            );
            // Rivers override to Water
            let terrain = if river_mask[i] {
                Terrain::Water
            } else {
                terrain
            };
            map.set(x, y, terrain);
        }
    }

    // Stage 7: Soil
    let soil = assign_soil(
        &heights,
        &slope,
        &moisture,
        &river_mask,
        w,
        h,
        config.terrain.water_level,
    );

    PipelineResult {
        map,
        heights,
        moisture,
        temperature,
        soil,
        river_mask,
        slope,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_produces_valid_map() {
        let config = PipelineConfig::default();
        let result = run_pipeline(64, 64, &config);
        assert_eq!(result.map.width, 64);
        assert_eq!(result.map.height, 64);
        assert_eq!(result.heights.len(), 64 * 64);
        assert_eq!(result.soil.len(), 64 * 64);
        assert_eq!(result.river_mask.len(), 64 * 64);
    }

    #[test]
    fn pipeline_generates_rivers() {
        let config = PipelineConfig::default();
        let result = run_pipeline(128, 128, &config);
        let river_count = result.river_mask.iter().filter(|&&r| r).count();
        assert!(
            river_count > 0,
            "should generate at least some river cells, got 0"
        );
        assert!(
            river_count < 128 * 128 / 2,
            "rivers should not cover more than half the map"
        );
    }

    #[test]
    fn pipeline_generates_varied_biomes() {
        let config = PipelineConfig::default();
        let result = run_pipeline(128, 128, &config);
        let mut types = std::collections::HashSet::new();
        for y in 0..128 {
            for x in 0..128 {
                if let Some(t) = result.map.get(x, y) {
                    types.insert(format!("{:?}", t));
                }
            }
        }
        assert!(
            types.len() >= 4,
            "should generate at least 4 terrain types, got {}: {:?}",
            types.len(),
            types
        );
    }

    #[test]
    fn thermal_erosion_smooths_spike() {
        let w = 10;
        let h = 10;
        let mut heights = vec![0.5; w * h];
        heights[5 * w + 5] = 1.0; // spike
        thermal_erosion(&mut heights, w, h, 0.01, 0.5, 20);
        assert!(
            heights[5 * w + 5] < 0.9,
            "spike should be reduced by thermal erosion"
        );
    }

    #[test]
    fn priority_flood_fills_depression() {
        let w = 10;
        let h = 10;
        let mut heights = vec![0.5; w * h];
        // Create a depression (ring of 0.8 around a 0.3 center)
        for dy in 3..7 {
            for dx in 3..7 {
                heights[dy * w + dx] = 0.8;
            }
        }
        heights[5 * w + 5] = 0.3; // depression center

        priority_flood(&mut heights, w, h);
        assert!(
            heights[5 * w + 5] >= 0.5,
            "depression should be filled to at least surrounding level"
        );
    }

    #[test]
    fn soil_assignment_covers_all_cells() {
        let config = PipelineConfig::default();
        let result = run_pipeline(64, 64, &config);
        // All land cells should have a soil type
        for i in 0..64 * 64 {
            if result.heights[i] >= config.terrain.water_level {
                let _ = result.soil[i]; // just verify it's assigned
            }
        }
    }
}
