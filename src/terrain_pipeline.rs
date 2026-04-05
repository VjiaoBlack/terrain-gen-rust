//! Terrain generation pipeline: transforms raw Perlin noise into realistic terrain
//! with cliffs, rivers, biomes, and soil.

use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;

use crate::terrain_gen::{self, TerrainGenConfig};
use crate::tilemap::{Terrain, TileMap};

// ─── Config ──────────────────────────────────────────────────────────────────

/// Which erosion model to use in the terrain pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ErosionModel {
    /// Analytical Stream Power Law — fast, incision-only, no deposition.
    Spl,
    /// SimpleHydrology particle-based — slower but produces meandering rivers,
    /// proper deposition, and realistic channel formation.
    SimpleHydrology,
    /// No erosion at all.
    Off,
}

impl Default for ErosionModel {
    fn default() -> Self {
        ErosionModel::SimpleHydrology
    }
}

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
    // Pipeline toggles
    pub erosion_model: ErosionModel,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            terrain: TerrainGenConfig {
                scale: 0.008, // halved from 0.015 — 2x larger terrain features
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
            erosion_model: ErosionModel::default(),
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

    /// How much fertility is lost per harvest. Richer soils resist depletion;
    /// poor soils exhaust quickly.
    pub fn harvest_depletion_rate(&self) -> f64 {
        match self {
            SoilType::Alluvial => 0.02, // 50 harvests to exhaust
            SoilType::Loam => 0.03,     // 33 harvests
            SoilType::Clay => 0.04,     // 25 harvests
            SoilType::Sand => 0.05,     // 20 harvests
            SoilType::Rocky => 0.08,    // 12 harvests
            SoilType::Peat => 0.04,     // 25 harvests (similar to clay)
        }
    }

    /// Bare ground foreground color — what you see with no vegetation.
    pub fn ground_fg(&self) -> crate::renderer::Color {
        use crate::renderer::Color;
        match self {
            SoilType::Sand => Color(170, 150, 100),  // pale tan
            SoilType::Loam => Color(90, 70, 40),     // medium brown
            SoilType::Alluvial => Color(65, 50, 30), // dark rich brown
            SoilType::Clay => Color(130, 100, 60),   // reddish-brown
            SoilType::Rocky => Color(120, 115, 105), // grey
            SoilType::Peat => Color(50, 40, 25),     // very dark brown
        }
    }

    /// Bare ground background color — darker variant.
    pub fn ground_bg(&self) -> crate::renderer::Color {
        use crate::renderer::Color;
        match self {
            SoilType::Sand => Color(140, 120, 78),
            SoilType::Loam => Color(60, 48, 28),
            SoilType::Alluvial => Color(42, 32, 18),
            SoilType::Clay => Color(95, 70, 40),
            SoilType::Rocky => Color(85, 80, 72),
            SoilType::Peat => Color(32, 25, 15),
        }
    }

    /// Base fertility ceiling for this soil type (used to cap recovery).
    pub fn base_fertility(&self) -> f64 {
        match self {
            SoilType::Alluvial => 1.0,
            SoilType::Loam => 0.85,
            SoilType::Peat => 0.75,
            SoilType::Clay => 0.70,
            SoilType::Sand => 0.40,
            SoilType::Rocky => 0.15,
        }
    }
}

impl Default for SoilType {
    fn default() -> Self {
        SoilType::Loam
    }
}

// ─── Resource Map ────────────────────────────────────────────────────────────

/// Per-tile geological resource potential, computed at world-gen time.
/// Values 0-255: higher = richer deposit at this location.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ResourcePotential {
    pub stone: u8,     // higher near mountains, cliffs, rocky soil
    pub wood: u8,      // higher in forests
    pub fertility: u8, // from soil yield_multiplier + river proximity
}

/// Precomputed resource map for the entire world. Ground-truth resource data
/// derived from terrain pipeline outputs (elevation, biome, soil, rivers).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceMap {
    pub width: usize,
    pub height: usize,
    pub potentials: Vec<ResourcePotential>,
}

impl ResourceMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            potentials: vec![ResourcePotential::default(); width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> &ResourcePotential {
        &self.potentials[y * self.width + x]
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut ResourcePotential {
        &mut self.potentials[y * self.width + x]
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
    pub resources: ResourceMap,
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

/// Hillslope diffusion: Laplacian smoothing that simulates soil creep.
/// Each land tile moves toward the average of its 4-connected neighbors.
/// Runs multiple iterations with a given diffusion rate per iteration.
/// Only modifies tiles above water_level (ocean floor stays flat).
///
/// This is the standard companion to SPL erosion — SPL handles channel
/// incision (rivers cutting into rock), hillslope diffusion handles
/// everything else (soil sliding downhill, ridges rounding, gullies filling).
pub fn hillslope_diffusion(
    heights: &mut [f64],
    w: usize,
    h: usize,
    water_level: f64,
    rate: f64,
    iterations: u32,
) {
    let n = w * h;
    let mut buf = vec![0.0f64; n];

    for _ in 0..iterations {
        buf.copy_from_slice(heights);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if heights[i] <= water_level {
                    continue;
                }
                let mut sum = 0.0;
                let mut count = 0.0;
                if x > 0 {
                    sum += buf[i - 1];
                    count += 1.0;
                }
                if x + 1 < w {
                    sum += buf[i + 1];
                    count += 1.0;
                }
                if y > 0 {
                    sum += buf[i - w];
                    count += 1.0;
                }
                if y + 1 < h {
                    sum += buf[i + w];
                    count += 1.0;
                }
                if count > 0.0 {
                    let avg = sum / count;
                    heights[i] += rate * (avg - heights[i]);
                    // Don't smooth below water level
                    if heights[i] < water_level {
                        heights[i] = water_level;
                    }
                }
            }
        }
    }
}

/// Sediment deposition: redistribute eroded material to low-slope areas.
///
/// SPL erosion removes material but doesn't deposit it anywhere. In nature,
/// eroded sediment accumulates at river mouths (deltas), in valleys (alluvial
/// plains), and wherever water slows down. This pass:
///
/// 1. Computes how much material SPL removed (needs pre-erosion heights)
/// 2. Distributes that material to nearby low-slope tiles
///
/// Simplified approach: find tiles adjacent to ocean with steep inland slopes
/// (these are the eroded river mouths) and build them up slightly, creating
/// a gentle transition instead of a cliff. Also fills narrow gullies by
/// averaging tiles that are much lower than their neighbors.
pub fn deposit_sediment(heights: &mut [f64], w: usize, h: usize, water_level: f64) {
    let n = w * h;
    let original = heights.to_vec();

    // Pass 1: Fill narrow gullies — tiles significantly lower than all neighbors
    // are likely erosion artifacts. Raise them toward the neighbor average.
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            if heights[i] <= water_level {
                continue;
            }
            let neighbors = [
                original[i - 1],
                original[i + 1],
                original[i - w],
                original[i + w],
            ];
            let min_neighbor = neighbors.iter().cloned().fold(f64::INFINITY, f64::min);
            let avg_neighbor = neighbors.iter().sum::<f64>() / 4.0;

            // If this tile is a narrow gully (lower than ALL neighbors by > threshold),
            // fill it partway toward the average
            let depth_below_min = min_neighbor - heights[i];
            if depth_below_min > 0.01 {
                // Fill 60% of the way to the minimum neighbor
                heights[i] += depth_below_min * 0.6;
            }
            // Also smooth tiles that are much lower than average
            let depth_below_avg = avg_neighbor - heights[i];
            if depth_below_avg > 0.02 {
                heights[i] += depth_below_avg * 0.3;
            }
        }
    }

    // Pass 2: Build gentle coastal transition — tiles just above water that
    // have very steep slopes toward ocean get a small deposit (delta formation)
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            if heights[i] <= water_level || heights[i] > water_level + 0.1 {
                continue; // only affect near-coast tiles
            }
            let neighbors = [i - 1, i + 1, i - w, i + w];
            let has_ocean_neighbor = neighbors
                .iter()
                .any(|&ni| original[ni] <= water_level);
            if has_ocean_neighbor {
                // Gentle deposit: raise slightly to smooth the land-ocean transition
                let target = water_level + 0.03;
                if heights[i] < target {
                    heights[i] = target;
                }
            }
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
                // Deposit to 4 nearest cells — but NOT below water level
                // (prevents silt from filling ocean basins flat)
                if height_here > config.terrain.water_level {
                    bilinear_add(heights, w, px, py, deposit);
                }
                // If below water level, sediment is "lost to the deep" (dropped)
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
        // Priority order: terrain-specific conditions first, then moisture-based
        if slope[i] > 0.12 {
            soil[i] = SoilType::Rocky;
        } else if heights[i] < water_level + 0.06 && moisture[i] > 0.3 {
            // Coastal low-elevation tiles are always sand — no peat on beaches
            soil[i] = SoilType::Sand;
        } else if moisture[i] > 0.85 && slope[i] < 0.02 {
            soil[i] = SoilType::Peat;
        } else if dist_to_river[i] < 4 && slope[i] < 0.05 {
            soil[i] = SoilType::Alluvial;
        } else if slope[i] < 0.04 && heights[i] < 0.5 {
            soil[i] = SoilType::Clay;
        }
        // else: Loam (default)
    }
    soil
}

// ─── Stage 8: Resource Map ───────────────────────────────────────────────────

/// Generate a precomputed resource map from terrain pipeline outputs.
/// Stone: high on Mountain tiles, moderate near Cliff, bonus from Perlin noise for veins.
/// Wood: matches Forest tiles, moderate near forest edges.
/// Fertility: from SoilType yield_multiplier + river proximity.
pub fn generate_resource_map(
    map: &TileMap,
    heights: &[f64],
    moisture: &[f64],
    slope: &[f64],
    soil: &[SoilType],
    river_mask: &[bool],
    w: usize,
    h: usize,
    seed: u32,
) -> ResourceMap {
    let stone_noise = Perlin::new(seed.wrapping_add(5000));
    let n = w * h;
    let mut resources = ResourceMap::new(w, h);

    // BFS distance from rivers for fertility calculation
    let mut dist_to_river = vec![u32::MAX; n];
    {
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
            if cd >= 12 {
                continue;
            }
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
    }

    for y_pos in 0..h {
        for x_pos in 0..w {
            let i = y_pos * w + x_pos;
            let terrain = match map.get(x_pos, y_pos) {
                Some(t) => *t,
                None => continue,
            };

            let pot = &mut resources.potentials[i];

            // ── Stone potential ──
            // Base: Mountain tiles get high stone, Cliff gets moderate
            let stone_base = match terrain {
                Terrain::Mountain => 180.0,
                Terrain::Cliff => 120.0,
                _ if soil[i] == SoilType::Rocky => 80.0,
                _ if slope[i] > 0.10 => 50.0,
                _ => 0.0,
            };
            if stone_base > 0.0 {
                // Add Perlin noise for vein variation (±40%)
                let noise_val = stone_noise.get([x_pos as f64 * 0.08, y_pos as f64 * 0.08]);
                let noise_mult = 0.6 + 0.4 * (noise_val + 1.0) / 2.0; // range [0.6, 1.0]
                // Elevation bonus: higher = richer
                let elev_bonus = 0.7 + 0.3 * heights[i].min(1.0);
                let stone_val = (stone_base * noise_mult * elev_bonus).round();
                pot.stone = (stone_val as u16).min(255) as u8;
            }

            // ── Wood potential ──
            // Forest tiles get high wood, edges get moderate
            let wood_base = match terrain {
                Terrain::Forest => 200.0,
                Terrain::Sapling => 60.0,
                _ => 0.0,
            };
            if wood_base > 0.0 {
                // Moisture bonus: wetter forests are denser
                let moisture_mult = 0.6 + 0.4 * moisture[i].min(1.0);
                let wood_val = (wood_base * moisture_mult).round();
                pot.wood = (wood_val as u16).min(255) as u8;
            }

            // ── Fertility potential ──
            // Based on soil yield_multiplier + river proximity
            let soil_yield = soil[i].yield_multiplier(); // 0.4 - 1.25
            let river_bonus = if dist_to_river[i] < 12 {
                1.0 - (dist_to_river[i] as f64 / 12.0) // 1.0 at river, 0.0 at dist 12
            } else {
                0.0
            };
            // Only compute fertility for potentially farmable terrain
            let is_farmable = matches!(
                terrain,
                Terrain::Grass
                    | Terrain::Sand
                    | Terrain::Forest
                    | Terrain::Marsh
                    | Terrain::Scrubland
            );
            if is_farmable {
                let fert_val = ((soil_yield * 0.6 + river_bonus * 0.4) * 255.0).round();
                pot.fertility = (fert_val as u16).min(255) as u8;
            }
        }
    }

    resources
}

// ─── Stage 8b: Ford Placement ───────────────────────────────────────────────

/// Place shallow fords at narrow river points with flat banks.
/// Fords are rare natural crossings — max ~1 per 30 river tiles.
/// Rules:
///   - River width <= 2.0 at that cell (narrow point)
///   - Both banks on opposite sides have slope < 0.04
///   - At most 1 ford per 30 tiles of river (measured by BFS distance between fords)
pub fn place_fords(
    map: &mut TileMap,
    river_mask: &[bool],
    river_width: &[f64],
    slope: &[f64],
    w: usize,
    h: usize,
) -> usize {
    let mut ford_count = 0usize;
    let mut ford_positions: Vec<(usize, usize)> = Vec::new();
    let min_ford_spacing = 30.0f64; // minimum distance between fords

    // Collect candidate ford cells: narrow river + flat banks
    let mut candidates: Vec<(usize, usize, f64)> = Vec::new(); // (x, y, width)
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            if !river_mask[i] {
                continue;
            }
            if river_width[i] > 2.0 {
                continue;
            }

            // Check for flat bank on both sides (look for non-river tiles with low slope)
            let dirs: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            let mut has_flat_bank_a = false;
            let mut has_flat_bank_b = false;
            for &(dx, dy) in &dirs {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                if !river_mask[ni] && slope[ni] < 0.04 {
                    if !has_flat_bank_a {
                        has_flat_bank_a = true;
                    } else {
                        has_flat_bank_b = true;
                    }
                }
            }
            if has_flat_bank_a && has_flat_bank_b {
                candidates.push((x, y, river_width[i]));
            }
        }
    }

    // Sort by width (prefer narrowest crossings)
    candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    // Place fords with spacing constraint
    for (cx, cy, _width) in &candidates {
        // Check minimum distance from existing fords
        let too_close = ford_positions.iter().any(|(fx, fy)| {
            let dx = *cx as f64 - *fx as f64;
            let dy = *cy as f64 - *fy as f64;
            (dx * dx + dy * dy).sqrt() < min_ford_spacing
        });
        if too_close {
            continue;
        }

        map.set(*cx, *cy, Terrain::Ford);
        ford_positions.push((*cx, *cy));
        ford_count += 1;
    }

    ford_count
}

// ─── Orchestrator ────────────────────────────────────────────────────────────

pub fn run_pipeline(w: usize, h: usize, config: &PipelineConfig) -> PipelineResult {
    // Stage 1: Base height (fBm)
    let (mut map, mut heights) = terrain_gen::generate_terrain(w, h, &config.terrain);

    // Stage 2: Terracing DISABLED — creates unnatural step patterns
    // apply_terraces(&mut heights, w, h, config);

    // Light thermal erosion to smooth spikes (5 iters, conservative)
    thermal_erosion(&mut heights, w, h, 0.05, 0.5, 5);

    // Stage 3: Priority flood first (SPL needs depression-free heightmap)
    priority_flood(&mut heights, w, h);

    // Stage 3b: Erosion — model selected by config
    match config.erosion_model {
        ErosionModel::Spl => {
            // Analytical SPL erosion — incision-only, no deposition.
            let spl_params = crate::analytical_erosion::SplParams {
                water_level: config.terrain.water_level,
                k: 0.0003,
                ..crate::analytical_erosion::SplParams::default()
            };
            crate::analytical_erosion::run_spl_erosion(&mut heights, w, h, &spl_params);
            // Hillslope diffusion + deposition to compensate for SPL's limitations
            hillslope_diffusion(&mut heights, w, h, config.terrain.water_level, 0.1, 8);
            deposit_sediment(&mut heights, w, h, config.terrain.water_level);
        }
        ErosionModel::SimpleHydrology => {
            // Nick McDonald's particle-based erosion — handles incision, deposition,
            // talus cascading, and momentum-driven meandering in one system.
            let hydro_params = crate::hydrology::HydroParams {
                water_level: config.terrain.water_level,
                ..crate::hydrology::HydroParams::default()
            };
            // 5 cycles of 8000 particles for 256x256 (scales with area)
            let area = w * h;
            let particles = ((area as f64 * 0.12) as u32).max(1000);
            crate::hydrology::run_hydrology(
                &mut heights, w, h, &hydro_params,
                5,          // cycles
                particles,
                config.terrain.seed,
            );
        }
        ErosionModel::Off => {
            // No erosion
        }
    }
    let river_mask = vec![false; w * h]; // empty — no pre-baked rivers

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

    // Stage 6b: Ford placement DISABLED — no pre-baked rivers
    // place_fords(&mut map, &river_mask, &river_width, &slope, w, h);

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

    // Stage 8: Resource map
    let resources = generate_resource_map(
        &map,
        &heights,
        &moisture,
        &slope,
        &soil,
        &river_mask,
        w,
        h,
        config.terrain.seed,
    );

    PipelineResult {
        map,
        heights,
        moisture,
        temperature,
        soil,
        river_mask,
        slope,
        resources,
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
    fn pipeline_river_mask_is_empty() {
        // Rivers disabled — river_mask should be all false
        let config = PipelineConfig::default();
        let result = run_pipeline(128, 128, &config);
        let river_count = result.river_mask.iter().filter(|&&r| r).count();
        assert_eq!(
            river_count, 0,
            "river_mask should be empty (rivers disabled)"
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
            types.len() >= 5,
            "128x128 map should generate at least 5 terrain types (we have 14 biome types), got {}: {:?}",
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

    #[test]
    fn resource_map_has_correct_dimensions() {
        let config = PipelineConfig::default();
        let result = run_pipeline(64, 64, &config);
        assert_eq!(result.resources.width, 64);
        assert_eq!(result.resources.height, 64);
        assert_eq!(result.resources.potentials.len(), 64 * 64);
    }

    #[test]
    fn resource_map_stone_correlates_with_mountains() {
        let config = PipelineConfig::default();
        let result = run_pipeline(128, 128, &config);
        let mut mountain_stone_sum = 0u64;
        let mut mountain_count = 0u64;
        let mut grass_stone_sum = 0u64;
        let mut grass_count = 0u64;
        for y in 0..128 {
            for x in 0..128 {
                let i = y * 128 + x;
                let pot = &result.resources.potentials[i];
                match result.map.get(x, y) {
                    Some(Terrain::Mountain) => {
                        mountain_stone_sum += pot.stone as u64;
                        mountain_count += 1;
                    }
                    Some(Terrain::Grass) => {
                        grass_stone_sum += pot.stone as u64;
                        grass_count += 1;
                    }
                    _ => {}
                }
            }
        }
        if mountain_count > 0 && grass_count > 0 {
            let mountain_avg = mountain_stone_sum as f64 / mountain_count as f64;
            let grass_avg = grass_stone_sum as f64 / grass_count as f64;
            assert!(
                mountain_avg > grass_avg,
                "mountain tiles should have higher avg stone ({}) than grass ({})",
                mountain_avg,
                grass_avg
            );
        }
    }

    #[test]
    fn resource_map_wood_correlates_with_forests() {
        let config = PipelineConfig::default();
        let result = run_pipeline(128, 128, &config);
        let mut forest_wood_sum = 0u64;
        let mut forest_count = 0u64;
        let mut non_forest_wood_sum = 0u64;
        let mut non_forest_count = 0u64;
        for y in 0..128 {
            for x in 0..128 {
                let i = y * 128 + x;
                let pot = &result.resources.potentials[i];
                match result.map.get(x, y) {
                    Some(Terrain::Forest) => {
                        forest_wood_sum += pot.wood as u64;
                        forest_count += 1;
                    }
                    Some(Terrain::Grass) | Some(Terrain::Sand) | Some(Terrain::Mountain) => {
                        non_forest_wood_sum += pot.wood as u64;
                        non_forest_count += 1;
                    }
                    _ => {}
                }
            }
        }
        if forest_count > 0 {
            let forest_avg = forest_wood_sum as f64 / forest_count as f64;
            assert!(
                forest_avg > 100.0,
                "forest tiles should have high avg wood ({})",
                forest_avg
            );
        }
        if non_forest_count > 0 {
            let non_forest_avg = non_forest_wood_sum as f64 / non_forest_count as f64;
            assert!(
                non_forest_avg < 10.0,
                "non-forest tiles should have low avg wood ({})",
                non_forest_avg
            );
        }
    }

    #[test]
    fn resource_map_fertility_near_rivers() {
        let config = PipelineConfig::default();
        let result = run_pipeline(128, 128, &config);
        let mut near_river_fert = 0u64;
        let mut near_river_count = 0u64;
        let mut far_fert = 0u64;
        let mut far_count = 0u64;
        for y in 0..128 {
            for x in 0..128 {
                let i = y * 128 + x;
                let pot = &result.resources.potentials[i];
                if pot.fertility == 0 {
                    continue;
                }
                // Check if river is within 3 tiles
                let mut near_river = false;
                for dy in -3i32..=3 {
                    for dx in -3i32..=3 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && nx < 128 && ny < 128 {
                            if result.river_mask[ny as usize * 128 + nx as usize] {
                                near_river = true;
                            }
                        }
                    }
                }
                if near_river {
                    near_river_fert += pot.fertility as u64;
                    near_river_count += 1;
                } else {
                    far_fert += pot.fertility as u64;
                    far_count += 1;
                }
            }
        }
        if near_river_count > 0 && far_count > 0 {
            let near_avg = near_river_fert as f64 / near_river_count as f64;
            let far_avg = far_fert as f64 / far_count as f64;
            assert!(
                near_avg > far_avg,
                "tiles near rivers should have higher avg fertility ({}) than far tiles ({})",
                near_avg,
                far_avg
            );
        }
    }

    #[test]
    fn resource_map_geographic_asymmetry() {
        // Two different seeds should produce meaningfully different resource distributions
        let config42 = PipelineConfig {
            terrain: crate::terrain_gen::TerrainGenConfig {
                seed: 42,
                scale: 0.015,
                ..Default::default()
            },
            ..PipelineConfig::default()
        };
        let config137 = PipelineConfig {
            terrain: crate::terrain_gen::TerrainGenConfig {
                seed: 137,
                scale: 0.015,
                ..Default::default()
            },
            ..PipelineConfig::default()
        };
        let r42 = run_pipeline(64, 64, &config42);
        let r137 = run_pipeline(64, 64, &config137);

        let sum_stone = |r: &PipelineResult| -> u64 {
            r.resources.potentials.iter().map(|p| p.stone as u64).sum()
        };
        let sum_wood = |r: &PipelineResult| -> u64 {
            r.resources.potentials.iter().map(|p| p.wood as u64).sum()
        };

        let s42 = sum_stone(&r42);
        let s137 = sum_stone(&r137);
        let w42 = sum_wood(&r42);
        let w137 = sum_wood(&r137);

        // At least one of stone or wood totals should differ by >10%
        let stone_diff = (s42 as f64 - s137 as f64).abs() / (s42.max(s137).max(1) as f64);
        let wood_diff = (w42 as f64 - w137 as f64).abs() / (w42.max(w137).max(1) as f64);
        assert!(
            stone_diff > 0.1 || wood_diff > 0.1,
            "seeds 42 and 137 should produce different resource distributions: \
             stone_diff={:.2}, wood_diff={:.2}",
            stone_diff,
            wood_diff
        );
    }

    // --- Resource map edge case tests ---

    #[test]
    fn resource_potential_water_tile_all_zeros() {
        use crate::tilemap::Terrain;
        let w = 10;
        let h = 10;
        let map = crate::tilemap::TileMap::new(w, h, Terrain::Water);
        let heights = vec![0.1; w * h]; // low elevation = water
        let moisture = vec![1.0; w * h];
        let slope = vec![0.0; w * h];
        let soil = vec![SoilType::Sand; w * h];
        let river_mask = vec![false; w * h];

        let resources = generate_resource_map(
            &map,
            &heights,
            &moisture,
            &slope,
            &soil,
            &river_mask,
            w,
            h,
            42,
        );
        let pot = resources.get(5, 5);
        assert_eq!(pot.stone, 0, "water tile should have 0 stone");
        assert_eq!(pot.wood, 0, "water tile should have 0 wood");
        assert_eq!(pot.fertility, 0, "water tile should have 0 fertility");
    }

    #[test]
    fn resource_potential_desert_low_wood_moderate_stone() {
        use crate::tilemap::Terrain;
        let w = 10;
        let h = 10;
        let map = crate::tilemap::TileMap::new(w, h, Terrain::Desert);
        let heights = vec![0.5; w * h];
        let moisture = vec![0.1; w * h];
        let slope = vec![0.0; w * h];
        let soil = vec![SoilType::Sand; w * h];
        let river_mask = vec![false; w * h];

        let resources = generate_resource_map(
            &map,
            &heights,
            &moisture,
            &slope,
            &soil,
            &river_mask,
            w,
            h,
            42,
        );
        let pot = resources.get(5, 5);
        assert_eq!(pot.wood, 0, "desert tile should have 0 wood");
        // Desert with Sand soil doesn't get stone unless slope > 0.1 or Rocky
        // so stone should be 0 in this flat case
        assert_eq!(
            pot.stone, 0,
            "flat desert with sand soil should have 0 stone"
        );
    }

    #[test]
    fn resource_potential_alluvial_soil_high_fertility() {
        use crate::tilemap::Terrain;
        let w = 10;
        let h = 10;
        let map = crate::tilemap::TileMap::new(w, h, Terrain::Grass);
        let heights = vec![0.5; w * h];
        let moisture = vec![0.5; w * h];
        let slope = vec![0.0; w * h];
        let mut soil = vec![SoilType::Rocky; w * h]; // low yield baseline
        let river_mask = vec![true; w * h]; // river everywhere for max bonus

        // Set one cell to Alluvial
        soil[5 * w + 5] = SoilType::Alluvial;

        let resources = generate_resource_map(
            &map,
            &heights,
            &moisture,
            &slope,
            &soil,
            &river_mask,
            w,
            h,
            42,
        );
        let alluvial_fert = resources.get(5, 5).fertility;
        let rocky_fert = resources.get(3, 3).fertility;

        assert!(
            alluvial_fert > rocky_fert,
            "alluvial soil should have higher fertility ({}) than rocky ({})",
            alluvial_fert,
            rocky_fert
        );
    }
}

#[cfg(test)]
mod height_diagnostics {
    use super::*;

    #[test]
    fn terrain_height_distribution() {
        let config = PipelineConfig::default();
        let result = run_pipeline(256, 256, &config);
        let n = result.heights.len();
        let water_level = config.terrain.water_level;

        let min_h = result.heights.iter().cloned().fold(f64::MAX, f64::min);
        let max_h = result.heights.iter().cloned().fold(f64::MIN, f64::max);
        let avg_h: f64 = result.heights.iter().sum::<f64>() / n as f64;

        let below_water = result.heights.iter().filter(|&&h| h < water_level).count();

        let mut sorted = result.heights.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        eprintln!("=== TERRAIN HEIGHT DIAGNOSTICS (seed default) ===");
        eprintln!("water_level: {water_level}");
        eprintln!("height range: {min_h:.3} to {max_h:.3}");
        eprintln!("avg height: {avg_h:.3}");
        eprintln!(
            "below water: {below_water} / {n} ({:.1}%)",
            below_water as f64 / n as f64 * 100.0
        );
        eprintln!("p10: {:.3}", sorted[n / 10]);
        eprintln!("p25: {:.3}", sorted[n / 4]);
        eprintln!("p50: {:.3}", sorted[n / 2]);
        eprintln!("p75: {:.3}", sorted[3 * n / 4]);
        eprintln!("p90: {:.3}", sorted[9 * n / 10]);

        // At least 5% of the map should be water
        assert!(
            below_water as f64 / n as f64 > 0.05,
            "Less than 5% water! below_water={below_water}, water_level={water_level}, avg={avg_h:.3}"
        );
    }
}

#[cfg(test)]
mod water_diagnostics {
    use super::*;
    use crate::tilemap::Terrain;

    #[test]
    fn water_tile_count_matches_height() {
        let config = PipelineConfig::default();
        let result = run_pipeline(256, 256, &config);
        let n = result.heights.len();
        let wl = config.terrain.water_level;

        let below_wl = result.heights.iter().filter(|&&h| h < wl).count();
        let water_terrain = (0..n)
            .filter(|&i| {
                let x = i % 256;
                let y = i / 256;
                result.map.get(x, y) == Some(&Terrain::Water)
            })
            .count();

        eprintln!("=== WATER DIAGNOSTICS ===");
        eprintln!(
            "below water_level ({wl}): {below_wl} ({:.1}%)",
            below_wl as f64 / n as f64 * 100.0
        );
        eprintln!(
            "Terrain::Water tiles: {water_terrain} ({:.1}%)",
            water_terrain as f64 / n as f64 * 100.0
        );
        eprintln!(
            "difference: {} tiles not classified as Water despite being below wl",
            below_wl as i64 - water_terrain as i64
        );

        // Water terrain count should roughly match below-water-level count
        assert!(
            water_terrain > 0,
            "ZERO water terrain tiles! below_wl={below_wl}"
        );
    }
}

#[cfg(test)]
mod biome_diagnostics {
    use super::*;

    #[test]
    fn biome_distribution() {
        let config = PipelineConfig::default();
        let result = run_pipeline(256, 256, &config);
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for y in 0..256 {
            for x in 0..256 {
                if let Some(t) = result.map.get(x, y) {
                    *counts.entry(format!("{:?}", t)).or_insert(0) += 1;
                }
            }
        }
        let mut sorted: Vec<_> = counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        eprintln!("=== BIOME DISTRIBUTION ===");
        for (name, count) in &sorted {
            eprintln!(
                "  {}: {} ({:.1}%)",
                name,
                count,
                *count as f64 / 65536.0 * 100.0
            );
        }
        // Check moisture distribution too
        let avg_moist: f64 = result.moisture.iter().sum::<f64>() / result.moisture.len() as f64;
        let zero_moist = result.moisture.iter().filter(|&&m| m < 0.01).count();
        eprintln!("avg pipeline moisture: {avg_moist:.3}");
        eprintln!(
            "zero moisture tiles: {zero_moist} ({:.1}%)",
            zero_moist as f64 / 65536.0 * 100.0
        );
    }

    #[test]
    fn hillslope_diffusion_smooths_spike() {
        let w = 8;
        let h = 8;
        let mut heights = vec![0.5f64; w * h];
        // Single spike in the middle
        heights[4 * w + 4] = 0.9;
        let original_spike = heights[4 * w + 4];

        hillslope_diffusion(&mut heights, w, h, 0.3, 0.2, 10);

        // Spike should be reduced
        assert!(
            heights[4 * w + 4] < original_spike,
            "spike should be smoothed: was {original_spike}, now {}",
            heights[4 * w + 4]
        );
        // Neighbors should have risen slightly
        assert!(
            heights[4 * w + 5] > 0.5,
            "neighbor should rise from diffusion"
        );
    }

    #[test]
    fn hillslope_diffusion_preserves_ocean() {
        let w = 8;
        let h = 8;
        let water_level = 0.35;
        let mut heights = vec![0.5f64; w * h];
        // Set ocean tiles
        for x in 0..3 {
            for y in 0..h {
                heights[y * w + x] = 0.3;
            }
        }
        let mut ocean_before = Vec::new();
        for y in 0..h {
            for x in 0..3 {
                ocean_before.push(heights[y * w + x]);
            }
        }

        hillslope_diffusion(&mut heights, w, h, water_level, 0.3, 20);

        let mut ocean_after = Vec::new();
        for y in 0..h {
            for x in 0..3 {
                ocean_after.push(heights[y * w + x]);
            }
        }
        assert_eq!(ocean_before, ocean_after, "ocean tiles should not change");
    }

    #[test]
    fn deposit_sediment_fills_narrow_gully() {
        let w = 8;
        let h = 8;
        let water_level = 0.3;
        let mut heights = vec![0.5f64; w * h];
        // Create a narrow gully: one tile much lower than neighbors
        heights[4 * w + 4] = 0.38;

        let before = heights[4 * w + 4];
        deposit_sediment(&mut heights, w, h, water_level);

        assert!(
            heights[4 * w + 4] > before,
            "gully should be partially filled: was {before}, now {}",
            heights[4 * w + 4]
        );
    }

    #[test]
    fn deposit_sediment_smooths_coastal_transition() {
        let w = 16;
        let h = 8;
        let water_level = 0.35;
        let mut heights = vec![0.5f64; w * h];
        // Ocean on left
        for y in 0..h {
            for x in 0..4 {
                heights[y * w + x] = 0.3;
            }
        }
        // Tile just above ocean (x=4) at barely above water level
        for y in 1..h - 1 {
            heights[y * w + 4] = 0.36;
        }

        deposit_sediment(&mut heights, w, h, water_level);

        // Coastal tile should be raised to at least water_level + 0.03
        for y in 1..h - 1 {
            assert!(
                heights[y * w + 4] >= water_level + 0.02,
                "coastal tile at y={y} should be raised, got {}",
                heights[y * w + 4]
            );
        }
    }

    /// Diagnostic: check soil type distribution after full pipeline.
    #[test]
    #[ignore] // run with: cargo test --lib diag_soil_distribution -- --nocapture --ignored
    fn diag_soil_distribution() {
        let config = PipelineConfig::default();
        let result = run_pipeline(256, 256, &config);
        let n = 256 * 256;

        let mut counts = std::collections::HashMap::new();
        for &s in &result.soil {
            *counts.entry(format!("{:?}", s)).or_insert(0u32) += 1;
        }

        eprintln!("=== Soil type distribution (128x128, seed {}) ===", config.terrain.seed);
        let mut sorted: Vec<_> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (name, count) in &sorted {
            eprintln!("  {:10}: {:5} ({:.1}%)", name, count, **count as f64 / n as f64 * 100.0);
        }

        // Also check moisture distribution near coast
        let water_level = config.terrain.water_level;
        let sz = 256;
        let mut coastal_moisture = Vec::new();
        for y in 0..sz {
            for x in 0..sz {
                let i = y * sz + x;
                if result.heights[i] > water_level && result.heights[i] < water_level + 0.1 {
                    coastal_moisture.push(result.moisture[i]);
                }
            }
        }
        if !coastal_moisture.is_empty() {
            let avg: f64 = coastal_moisture.iter().sum::<f64>() / coastal_moisture.len() as f64;
            let max = coastal_moisture.iter().cloned().fold(0.0f64, f64::max);
            let above_95 = coastal_moisture.iter().filter(|&&m| m > 0.95).count();
            eprintln!("\nCoastal tiles (height < water_level + 0.1):");
            eprintln!("  count: {}", coastal_moisture.len());
            eprintln!("  avg moisture: {:.3}", avg);
            eprintln!("  max moisture: {:.3}", max);
            eprintln!("  above 0.95: {} ({:.1}%)", above_95, above_95 as f64 / coastal_moisture.len() as f64 * 100.0);
        }
    }
}
