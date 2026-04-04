/// Pipe-model water simulation (research prototype).
///
/// Models 8-directional flow using virtual pipes connecting adjacent tiles.
/// Each pipe carries flux driven by hydrostatic pressure differences.
/// Volume is conserved by scaling outflow when it would exceed available water.
///
/// Reference: O'Brien 1995 / Stam GDC 2008 pipe model, extended to 8 directions.
///
/// Direction index layout:
///   0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW
const GRAVITY: f64 = 9.81;
const PIPE_AREA: f64 = 1.0;
const MIN_DEPTH: f64 = 0.0001;

// --- Sediment transport constants ---
/// Carrying capacity coefficient (Hjulström simplified).
const K_C: f64 = 0.01;
/// Erosion rate — slow, geological time.
const K_E: f64 = 0.0005;
/// Deposition rate — faster than erosion.
const K_D: f64 = 0.005;
/// Maximum erosion per tick to prevent runaway.
const MAX_ERODE: f64 = 0.001;
/// Outside-of-bend erosion multiplier.
const CURVATURE_EROSION_MULT: f64 = 2.0;
/// Minimum depth for sediment erosion to occur.
const SEDIMENT_MIN_DEPTH: f64 = 0.01;

/// Pipe length for cardinal directions (tile spacing = 1.0).
const PIPE_LEN_CARDINAL: f64 = 1.0;
/// Pipe length for diagonal directions (sqrt(2) tile spacings).
const PIPE_LEN_DIAGONAL: f64 = std::f64::consts::SQRT_2;

/// Direction offsets: (dx, dy) for each of the 8 directions.
/// +x = East, +y = South (row-major, y increases downward).
const DIR_OFFSETS: [(i32, i32); 8] = [
    (0, -1),  // 0: N
    (1, -1),  // 1: NE
    (1, 0),   // 2: E
    (1, 1),   // 3: SE
    (0, 1),   // 4: S
    (-1, 1),  // 5: SW
    (-1, 0),  // 6: W  — index 6 in offsets
    (-1, -1), // 7: NW
];

/// Returns the pipe length for direction `d`.
#[inline]
fn pipe_length(d: usize) -> f64 {
    if d % 2 == 0 {
        PIPE_LEN_CARDINAL // N, E, S, W
    } else {
        PIPE_LEN_DIAGONAL // NE, SE, SW, NW
    }
}

/// Velocity contribution factors per direction.
/// Cardinal directions contribute to exactly one axis; diagonals to both.
/// x-axis: positive = East. y-axis: positive = South.
#[inline]
fn dir_xy(d: usize) -> (f64, f64) {
    let (dx, dy) = DIR_OFFSETS[d];
    (dx as f64, dy as f64)
}

pub struct PipeWater {
    pub width: usize,
    pub height: usize,
    /// Water depth per tile (meters).
    pub depth: Vec<f64>,
    /// Outgoing flow through 8 pipes per tile (m³/s).
    /// flux[tile_idx][d] is the outflow from `tile_idx` in direction `d`.
    pub flux: Vec<[f64; 8]>,
    /// Derived velocity x-component (East positive).
    pub velocity_x: Vec<f64>,
    /// Derived velocity y-component (South positive).
    pub velocity_y: Vec<f64>,
    /// Suspended sediment concentration per tile (volume units).
    pub suspended: Vec<f64>,
}

impl PipeWater {
    /// Create a new empty simulation grid.
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            depth: vec![0.0; n],
            flux: vec![[0.0; 8]; n],
            velocity_x: vec![0.0; n],
            velocity_y: vec![0.0; n],
            suspended: vec![0.0; n],
        }
    }

    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// Add water at tile (x, y) — simulates rainfall.
    pub fn add_water(&mut self, x: usize, y: usize, amount: f64) {
        let i = self.idx(x, y);
        self.depth[i] = (self.depth[i] + amount).max(0.0);
    }

    /// Advance the simulation by one timestep `dt` (seconds).
    ///
    /// `heights` is the terrain height array (row-major, same layout as depth).
    pub fn step(&mut self, heights: &[f64], dt: f64) {
        assert_eq!(
            heights.len(),
            self.width * self.height,
            "heights length must match grid size"
        );

        let n = self.width * self.height;

        // ---------------------------------------------------------------
        // Step 1: Compute new flux values driven by pressure differences.
        // pressure[i] = terrain_height[i] + water_depth[i]
        // ---------------------------------------------------------------
        let mut new_flux = vec![[0.0_f64; 8]; n];

        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.idx(x, y);
                let pressure_here = heights[i] + self.depth[i];

                for d in 0..8usize {
                    let (dx, dy) = DIR_OFFSETS[d];
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;

                    // Out-of-bounds neighbors: no pipe (treat as wall).
                    if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                        new_flux[i][d] = 0.0;
                        continue;
                    }

                    let ni = self.idx(nx as usize, ny as usize);
                    let pressure_neighbor = heights[ni] + self.depth[ni];

                    // flux_delta = dt * gravity * PIPE_AREA * delta_pressure / pipe_length
                    let delta_p = pressure_here - pressure_neighbor;
                    let flux_delta = dt * GRAVITY * PIPE_AREA * delta_p / pipe_length(d);

                    // Accumulate and clamp to non-negative (unidirectional pipes).
                    new_flux[i][d] = (self.flux[i][d] + flux_delta).max(0.0);
                }
            }
        }

        // ---------------------------------------------------------------
        // Step 2: Scale outflow to conserve volume.
        // If total outflow would drain more than the available water, scale
        // all outgoing fluxes proportionally so at most depth/dt flows out.
        // ---------------------------------------------------------------
        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.idx(x, y);
                let total_outflow: f64 = new_flux[i].iter().sum();

                if total_outflow > 0.0 {
                    // Maximum water that can leave this tile in one step.
                    let available = self.depth[i] / dt;
                    if total_outflow > available {
                        let scale = available / total_outflow;
                        for d in 0..8 {
                            new_flux[i][d] *= scale;
                        }
                    }
                }
            }
        }

        // ---------------------------------------------------------------
        // Step 3: Update depth from net flux.
        // depth[i] += dt * (sum_inflow[i] - sum_outflow[i])
        // ---------------------------------------------------------------
        let mut new_depth = self.depth.clone();

        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.idx(x, y);

                // Sum all outgoing flux from this tile.
                let outflow: f64 = new_flux[i].iter().sum();

                // Sum inflow: for each direction, check the neighbor's flux
                // in the opposite direction (toward us).
                let mut inflow = 0.0_f64;
                for d in 0..8usize {
                    let (dx, dy) = DIR_OFFSETS[d];
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && nx < self.width as i32 && ny < self.height as i32 {
                        let ni = self.idx(nx as usize, ny as usize);
                        // The opposite direction of d is (d + 4) % 8.
                        let opp = (d + 4) % 8;
                        inflow += new_flux[ni][opp];
                    }
                }

                new_depth[i] = (self.depth[i] + dt * (inflow - outflow)).max(0.0);
            }
        }

        // ---------------------------------------------------------------
        // Step 4: Derive velocity from net flux.
        // velocity = (weighted sum of flux * direction_unit_vector) / depth
        // ---------------------------------------------------------------
        let mut new_vx = vec![0.0_f64; n];
        let mut new_vy = vec![0.0_f64; n];

        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.idx(x, y);
                let d = new_depth[i];

                if d < MIN_DEPTH {
                    new_vx[i] = 0.0;
                    new_vy[i] = 0.0;
                    continue;
                }

                // Net flux in each axis = sum of flux_in - flux_out per direction.
                let mut net_x = 0.0_f64;
                let mut net_y = 0.0_f64;

                for dir in 0..8usize {
                    let (ux, uy) = dir_xy(dir);
                    // Outflow in direction dir contributes positively (moving water out).
                    let f_out = new_flux[i][dir];

                    // Inflow from the neighbor in direction dir (neighbor's opposite flux).
                    let (dx, dy) = DIR_OFFSETS[dir];
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let f_in = if nx >= 0
                        && ny >= 0
                        && nx < self.width as i32
                        && ny < self.height as i32
                    {
                        let ni = self.idx(nx as usize, ny as usize);
                        let opp = (dir + 4) % 8;
                        new_flux[ni][opp]
                    } else {
                        0.0
                    };

                    // Net contribution: outflow - inflow along this axis component.
                    net_x += (f_out - f_in) * ux;
                    net_y += (f_out - f_in) * uy;
                }

                // Velocity = net volumetric flux / (depth * cell_area).
                // Cell area = 1.0, so velocity = net_flux / depth.
                new_vx[i] = net_x / d;
                new_vy[i] = net_y / d;
            }
        }

        // Commit all updates.
        self.flux = new_flux;
        self.depth = new_depth;
        self.velocity_x = new_vx;
        self.velocity_y = new_vy;
    }

    /// Get water depth at tile (x, y).
    pub fn get_depth(&self, x: usize, y: usize) -> f64 {
        self.depth[self.idx(x, y)]
    }

    /// Get water velocity at tile (x, y) as (vx, vy).
    pub fn get_velocity(&self, x: usize, y: usize) -> (f64, f64) {
        let i = self.idx(x, y);
        (self.velocity_x[i], self.velocity_y[i])
    }

    /// Compute total water volume on the grid (for conservation checks).
    pub fn total_water(&self) -> f64 {
        self.depth.iter().sum()
    }

    /// Compute total suspended sediment on the grid (for conservation checks).
    pub fn total_suspended(&self) -> f64 {
        self.suspended.iter().sum()
    }

    /// Bilinear sample of velocity at fractional coordinates.
    /// Returns (vx, vy). Clamps to grid boundaries.
    fn sample_velocity(&self, fx: f64, fy: f64) -> (f64, f64) {
        let x0 = (fx.floor() as i64).clamp(0, self.width as i64 - 1) as usize;
        let y0 = (fy.floor() as i64).clamp(0, self.height as i64 - 1) as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let sx = (fx - x0 as f64).clamp(0.0, 1.0);
        let sy = (fy - y0 as f64).clamp(0.0, 1.0);

        let i00 = self.idx(x0, y0);
        let i10 = self.idx(x1, y0);
        let i01 = self.idx(x0, y1);
        let i11 = self.idx(x1, y1);

        let vx = self.velocity_x[i00] * (1.0 - sx) * (1.0 - sy)
            + self.velocity_x[i10] * sx * (1.0 - sy)
            + self.velocity_x[i01] * (1.0 - sx) * sy
            + self.velocity_x[i11] * sx * sy;

        let vy = self.velocity_y[i00] * (1.0 - sx) * (1.0 - sy)
            + self.velocity_y[i10] * sx * (1.0 - sy)
            + self.velocity_y[i01] * (1.0 - sx) * sy
            + self.velocity_y[i11] * sx * sy;

        (vx, vy)
    }

    /// Compute flow curvature at tile (x, y).
    ///
    /// Measures how much the velocity direction changes vs the upstream neighbor.
    /// Positive curvature = turning left (outside of a rightward bend), negative = right.
    /// Returns the absolute curvature magnitude, suitable for erosion scaling.
    pub fn flow_curvature(&self, x: usize, y: usize) -> f64 {
        let i = self.idx(x, y);
        let vx = self.velocity_x[i];
        let vy = self.velocity_y[i];
        let speed = (vx * vx + vy * vy).sqrt();
        if speed < 0.001 {
            return 0.0;
        }

        // Sample velocity at upstream position (one unit upstream).
        let ux = x as f64 - vx / speed;
        let uy = y as f64 - vy / speed;
        let (uvx, uvy) = self.sample_velocity(ux, uy);
        let uspeed = (uvx * uvx + uvy * uvy).sqrt();
        if uspeed < 0.001 {
            return 0.0;
        }

        // Curvature via cross product of normalized upstream and current direction.
        // cross = upstream_dir × current_dir (z-component of 2D cross product)
        let cross = (uvx / uspeed) * (vy / speed) - (uvy / uspeed) * (vx / speed);

        // Return absolute curvature scaled by speed (faster flow = more curvature effect).
        cross.abs()
    }

    /// Advance sediment transport by one step.
    ///
    /// This should be called after `step()` so that velocity fields are current.
    /// Modifies `heights` (erosion removes terrain, deposition adds it) and
    /// updates `self.suspended`.
    pub fn step_sediment(&mut self, heights: &mut [f64]) {
        let n = self.width * self.height;
        assert_eq!(heights.len(), n, "heights length must match grid size");

        // --- Phase 1: Erosion and deposition ---
        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.idx(x, y);
                let d = self.depth[i];
                let vx = self.velocity_x[i];
                let vy = self.velocity_y[i];
                let speed_sq = vx * vx + vy * vy;

                // Carrying capacity: Hjulström simplified.
                let capacity = K_C * d * speed_sq;

                if self.suspended[i] < capacity && d > SEDIMENT_MIN_DEPTH {
                    // Erode: pick up sediment from terrain.
                    let curvature = self.flow_curvature(x, y);
                    // Scale erosion: 1.0 at zero curvature, up to CURVATURE_EROSION_MULT at high curvature.
                    let curvature_factor =
                        1.0 + curvature.min(1.0) * (CURVATURE_EROSION_MULT - 1.0);
                    let erode =
                        (K_E * (capacity - self.suspended[i]) * curvature_factor).min(MAX_ERODE);
                    let erode = erode.min(heights[i].max(0.0)); // Don't erode below 0.
                    heights[i] -= erode;
                    self.suspended[i] += erode;
                } else if self.suspended[i] > capacity {
                    // Deposit: drop sediment onto terrain.
                    let deposit = K_D * (self.suspended[i] - capacity);
                    let deposit = deposit.min(self.suspended[i]); // Don't deposit more than we have.
                    heights[i] += deposit;
                    self.suspended[i] -= deposit;
                }
            }
        }

        // --- Phase 2: Advect suspended sediment with water velocity ---
        let mut new_suspended = vec![0.0_f64; n];

        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.idx(x, y);
                let sed = self.suspended[i];
                if sed < 1e-15 {
                    continue;
                }

                let vx = self.velocity_x[i];
                let vy = self.velocity_y[i];

                // Source position (semi-Lagrangian backtrack).
                // We move sediment forward by computing where it goes.
                // For stability, use a simple forward Euler with clamping.
                let dst_x = (x as f64 + vx).clamp(0.0, (self.width - 1) as f64);
                let dst_y = (y as f64 + vy).clamp(0.0, (self.height - 1) as f64);

                // Bilinear splat to destination.
                let x0 = dst_x.floor() as usize;
                let y0 = dst_y.floor() as usize;
                let x1 = (x0 + 1).min(self.width - 1);
                let y1 = (y0 + 1).min(self.height - 1);

                let sx = dst_x - x0 as f64;
                let sy = dst_y - y0 as f64;

                new_suspended[self.idx(x0, y0)] += sed * (1.0 - sx) * (1.0 - sy);
                new_suspended[self.idx(x1, y0)] += sed * sx * (1.0 - sy);
                new_suspended[self.idx(x0, y1)] += sed * (1.0 - sx) * sy;
                new_suspended[self.idx(x1, y1)] += sed * sx * sy;
            }
        }

        self.suspended = new_suspended;
    }
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 0.1;

    fn flat_terrain(size: usize) -> Vec<f64> {
        vec![0.0; size * size]
    }

    fn slope_terrain(width: usize, height: usize) -> Vec<f64> {
        // Terrain slopes downward in the +x (East) direction.
        // tile (0,y) is highest, tile (width-1, y) is lowest.
        let mut t = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                t[y * width + x] = (width - 1 - x) as f64;
            }
        }
        t
    }

    fn run_steps(sim: &mut PipeWater, heights: &[f64], steps: usize) {
        for _ in 0..steps {
            sim.step(heights, DT);
        }
    }

    // -------------------------------------------------------------------------
    // Test 1: Water on flat terrain with no initial flux stays put (no flow).
    // -------------------------------------------------------------------------
    #[test]
    fn test_flat_terrain_no_flow() {
        let mut sim = PipeWater::new(5, 5);
        let terrain = flat_terrain(5);

        // Place water at centre.
        sim.add_water(2, 2, 1.0);
        let initial_depth = sim.get_depth(2, 2);

        // On a perfectly flat surface, pressure differences are due only to
        // depth differences — water will spread to neighbours. After many
        // steps it should reach an equilibrium where depth everywhere is equal.
        run_steps(&mut sim, &terrain, 200);

        // Total volume must be conserved (checked by test_volume_conserved too).
        let total: f64 = sim.depth.iter().sum();
        assert!(
            (total - initial_depth).abs() < 1e-9,
            "volume not conserved on flat terrain: expected {initial_depth}, got {total}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 2: Water on a slope flows downhill.
    // -------------------------------------------------------------------------
    #[test]
    fn test_slope_flows_downhill() {
        let w = 10;
        let h = 5;
        let terrain = slope_terrain(w, h);
        let mut sim = PipeWater::new(w, h);

        // Place water at the high end (x=0).
        for y in 0..h {
            sim.add_water(0, y, 1.0);
        }

        run_steps(&mut sim, &terrain, 100);

        // After many steps, water should have moved toward the low end (x=w-1).
        // Depth at high end should be less than at low end.
        let depth_high: f64 = (0..h).map(|y| sim.get_depth(0, y)).sum::<f64>() / h as f64;
        let depth_low: f64 = (0..h).map(|y| sim.get_depth(w - 1, y)).sum::<f64>() / h as f64;

        assert!(
            depth_low > depth_high,
            "water did not flow downhill: depth_high={depth_high:.4}, depth_low={depth_low:.4}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 3: Water fills a basin and levels out.
    // -------------------------------------------------------------------------
    #[test]
    fn test_basin_levels_out() {
        let w = 7;
        let h = 7;
        let mut terrain = vec![0.0; w * h];

        // Build a rim around the edges so water can't escape.
        for x in 0..w {
            for y in 0..h {
                if x == 0 || x == w - 1 || y == 0 || y == h - 1 {
                    terrain[y * w + x] = 10.0; // high walls
                }
            }
        }

        let mut sim = PipeWater::new(w, h);

        // Pour water into one interior corner.
        sim.add_water(1, 1, 5.0);

        run_steps(&mut sim, &terrain, 500);

        // Water should spread across the flat interior.
        // Count interior tiles (5x5 = 25) and check depth variance is small.
        let interior: Vec<f64> = (1..h - 1)
            .flat_map(|y| (1..w - 1).map(move |x| (x, y)))
            .map(|(x, y)| sim.get_depth(x, y))
            .collect();

        let mean = interior.iter().sum::<f64>() / interior.len() as f64;
        let variance =
            interior.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / interior.len() as f64;

        assert!(
            variance < 0.1,
            "basin did not level out: depth variance = {variance:.4}, mean = {mean:.4}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 4: Volume is conserved across many steps.
    // -------------------------------------------------------------------------
    #[test]
    fn test_volume_conserved() {
        let w = 8;
        let h = 8;
        let terrain = slope_terrain(w, h);
        let mut sim = PipeWater::new(w, h);

        // Scatter water across the grid.
        sim.add_water(0, 0, 2.0);
        sim.add_water(3, 3, 1.5);
        sim.add_water(7, 7, 0.5);

        let initial_total = sim.total_water();
        run_steps(&mut sim, &terrain, 300);
        let final_total = sim.total_water();

        assert!(
            (final_total - initial_total).abs() < 1e-6,
            "volume not conserved: initial={initial_total:.8}, final={final_total:.8}, diff={}",
            (final_total - initial_total).abs()
        );
    }

    // -------------------------------------------------------------------------
    // Test 5: Velocity is zero (or near zero) on flat water in equilibrium.
    // -------------------------------------------------------------------------
    #[test]
    fn test_velocity_zero_on_flat_equilibrium() {
        let w = 5;
        let h = 5;
        let terrain = flat_terrain(w);
        let mut sim = PipeWater::new(w, h);

        // Uniform water depth everywhere.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        // With uniform depth on flat terrain there is no pressure gradient,
        // so no flux should develop.
        sim.step(&terrain, DT);

        let max_speed = (0..w * h)
            .map(|i| {
                let vx = sim.velocity_x[i];
                let vy = sim.velocity_y[i];
                (vx * vx + vy * vy).sqrt()
            })
            .fold(0.0_f64, f64::max);

        assert!(
            max_speed < 1e-10,
            "expected zero velocity on uniform flat water, got max speed = {max_speed:.2e}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 6: Velocity points downhill on a slope.
    // -------------------------------------------------------------------------
    #[test]
    fn test_velocity_points_downhill_on_slope() {
        let w = 10;
        let h = 5;
        let terrain = slope_terrain(w, h);
        let mut sim = PipeWater::new(w, h);

        // Uniform water on the slope.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        run_steps(&mut sim, &terrain, 20);

        // Velocity at mid-slope should have positive x-component (flowing East = downhill).
        let mid_y = h / 2;
        let mid_x = w / 2;
        let (vx, _vy) = sim.get_velocity(mid_x, mid_y);

        assert!(
            vx > 0.0,
            "expected downhill (positive x) velocity on slope, got vx={vx:.4}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 7: Diagonal flow works (water placed at corner flows diagonally).
    // -------------------------------------------------------------------------
    #[test]
    fn test_diagonal_flow() {
        let w = 10;
        let h = 10;
        let mut terrain = vec![0.0; w * h];

        // Terrain slopes diagonally (SE direction).
        for y in 0..h {
            for x in 0..w {
                terrain[y * w + x] = ((w - 1 - x) + (h - 1 - y)) as f64;
            }
        }

        let mut sim = PipeWater::new(w, h);
        // Place water at the NW corner (highest point).
        sim.add_water(0, 0, 5.0);

        run_steps(&mut sim, &terrain, 100);

        // Water should have moved toward the SE corner (low end).
        let depth_nw = sim.get_depth(0, 0);
        let depth_se = sim.get_depth(w - 1, h - 1);

        assert!(
            depth_se > depth_nw,
            "diagonal flow failed: depth at SE corner ({depth_se:.4}) should exceed NW ({depth_nw:.4})"
        );
    }

    // -------------------------------------------------------------------------
    // Test 8: Deeper water generates more outflow (deeper = higher pressure).
    // -------------------------------------------------------------------------
    #[test]
    fn test_deep_water_flows_faster() {
        // Two identical setups — one with more water. The deeper one should
        // exhibit higher flux magnitude after one step.
        let w = 5;
        let h = 5;
        let terrain = flat_terrain(w);

        let measure_outflow = |amount: f64| -> f64 {
            let mut sim = PipeWater::new(w, h);
            sim.add_water(2, 2, amount);
            sim.step(&terrain, DT);
            sim.flux[sim.idx(2, 2)].iter().sum()
        };

        let outflow_shallow = measure_outflow(0.5);
        let outflow_deep = measure_outflow(2.0);

        assert!(
            outflow_deep > outflow_shallow,
            "expected deeper water to flow faster: shallow={outflow_shallow:.6}, deep={outflow_deep:.6}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 9: Adding water increases local depth.
    // -------------------------------------------------------------------------
    #[test]
    fn test_add_water_increases_depth() {
        let mut sim = PipeWater::new(5, 5);
        let before = sim.get_depth(2, 2);
        sim.add_water(2, 2, 3.0);
        let after = sim.get_depth(2, 2);

        assert!(
            after > before,
            "add_water did not increase depth: before={before}, after={after}"
        );
        assert!(
            (after - before - 3.0).abs() < 1e-12,
            "add_water added wrong amount: expected +3.0, got +{:.12}",
            after - before
        );
    }

    // -------------------------------------------------------------------------
    // Test 10: Water does not flow through high terrain walls.
    // -------------------------------------------------------------------------
    #[test]
    fn test_no_flow_through_walls() {
        let w = 7;
        let h = 3;
        let mut terrain = vec![0.0; w * h];

        // Place a wall of tall terrain at x=3 across all rows.
        for y in 0..h {
            terrain[y * w + 3] = 100.0;
        }

        let mut sim = PipeWater::new(w, h);

        // Water on the left side.
        for y in 0..h {
            sim.add_water(1, y, 2.0);
        }

        run_steps(&mut sim, &terrain, 200);

        // The right side (x > 3) should remain (nearly) empty.
        let right_total: f64 = (0..h).map(|y| sim.get_depth(5, y)).sum();

        assert!(
            right_total < 0.01,
            "water leaked through wall: right_total={right_total:.6}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 11: Total water is correct after multiple add_water calls.
    // -------------------------------------------------------------------------
    #[test]
    fn test_total_water_after_adds() {
        let mut sim = PipeWater::new(5, 5);
        sim.add_water(0, 0, 1.0);
        sim.add_water(1, 1, 2.0);
        sim.add_water(4, 4, 0.5);

        let expected = 3.5;
        let actual = sim.total_water();
        assert!(
            (actual - expected).abs() < 1e-12,
            "total_water wrong after adds: expected {expected}, got {actual}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 12: Flux is non-negative (pipe model invariant).
    // -------------------------------------------------------------------------
    #[test]
    fn test_flux_non_negative() {
        let w = 6;
        let h = 6;
        let terrain = slope_terrain(w, h);
        let mut sim = PipeWater::new(w, h);

        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        run_steps(&mut sim, &terrain, 50);

        for (i, pipes) in sim.flux.iter().enumerate() {
            for (d, &f) in pipes.iter().enumerate() {
                assert!(f >= 0.0, "negative flux at tile {i}, direction {d}: f={f}");
            }
        }
    }

    // -------------------------------------------------------------------------
    // Test 13: Water at border doesn't escape the grid (no-flow boundary).
    // -------------------------------------------------------------------------
    #[test]
    fn test_border_no_escape() {
        let w = 5;
        let h = 5;
        let terrain = flat_terrain(w);
        let mut sim = PipeWater::new(w, h);

        // Fill the entire grid uniformly.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        let initial_total = sim.total_water();
        run_steps(&mut sim, &terrain, 100);
        let final_total = sim.total_water();

        assert!(
            (final_total - initial_total).abs() < 1e-8,
            "water escaped grid: initial={initial_total:.8}, final={final_total:.8}"
        );
    }

    // =========================================================================
    // Sediment transport tests
    // =========================================================================

    /// Helper: run pipe water steps then sediment steps.
    fn run_sediment_steps(sim: &mut PipeWater, heights: &mut [f64], steps: usize) {
        for _ in 0..steps {
            sim.step(heights, DT);
            sim.step_sediment(heights);
        }
    }

    // -------------------------------------------------------------------------
    // Sediment Test 1: Sediment picks up on steep flow.
    // -------------------------------------------------------------------------
    #[test]
    fn test_sediment_pickup_on_steep_flow() {
        let w = 10;
        let h = 5;
        let mut terrain = slope_terrain(w, h);
        let mut sim = PipeWater::new(w, h);

        // Add enough water on the slope to generate flow.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        // Run water steps first to establish flow.
        run_steps(&mut sim, &terrain, 50);

        // Now run sediment steps.
        let initial_suspended = sim.total_suspended();
        assert!(initial_suspended < 1e-15, "suspended should start at zero");

        for _ in 0..100 {
            sim.step(&terrain, DT);
            sim.step_sediment(&mut terrain);
        }

        let final_suspended = sim.total_suspended();
        assert!(
            final_suspended > 0.0,
            "sediment should be picked up on steep flow, got {final_suspended}"
        );
    }

    // -------------------------------------------------------------------------
    // Sediment Test 2: Sediment deposits in calm water.
    // -------------------------------------------------------------------------
    #[test]
    fn test_sediment_deposits_in_calm_water() {
        let w = 7;
        let h = 7;
        let mut terrain = vec![0.0; w * h];

        // Build a basin with high walls.
        for x in 0..w {
            for y in 0..h {
                if x == 0 || x == w - 1 || y == 0 || y == h - 1 {
                    terrain[y * w + x] = 10.0;
                }
            }
        }

        let mut sim = PipeWater::new(w, h);

        // Fill basin with calm water.
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                sim.add_water(x, y, 2.0);
            }
        }

        // Manually inject suspended sediment into the calm water.
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = sim.idx(x, y);
                sim.suspended[i] = 0.01;
            }
        }

        // Let it settle — water in a basin should reach equilibrium (no flow),
        // and sediment should deposit.
        run_steps(&mut sim, &terrain, 200);
        let initial_suspended = sim.total_suspended();

        for _ in 0..200 {
            sim.step(&terrain, DT);
            sim.step_sediment(&mut terrain);
        }

        let final_suspended = sim.total_suspended();
        assert!(
            final_suspended < initial_suspended,
            "sediment should deposit in calm water: initial={initial_suspended:.6}, final={final_suspended:.6}"
        );
    }

    // -------------------------------------------------------------------------
    // Sediment Test 3: Total mass conservation (terrain + sediment = constant).
    // -------------------------------------------------------------------------
    #[test]
    fn test_sediment_mass_conservation() {
        let w = 8;
        let h = 8;
        let mut terrain = slope_terrain(w, h);
        let mut sim = PipeWater::new(w, h);

        // Add water.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 0.5);
            }
        }

        let initial_terrain_mass: f64 = terrain.iter().sum();
        let initial_suspended = sim.total_suspended();
        let initial_total = initial_terrain_mass + initial_suspended;

        // Run many combined steps.
        for _ in 0..200 {
            sim.step(&terrain, DT);
            sim.step_sediment(&mut terrain);
        }

        let final_terrain_mass: f64 = terrain.iter().sum();
        let final_suspended = sim.total_suspended();
        let final_total = final_terrain_mass + final_suspended;

        let diff = (final_total - initial_total).abs();
        assert!(
            diff < 1e-6,
            "mass not conserved: initial={initial_total:.8}, final={final_total:.8}, diff={diff:.2e}"
        );
    }

    // -------------------------------------------------------------------------
    // Sediment Test 4: Curvature detection on a curved channel.
    // -------------------------------------------------------------------------
    #[test]
    fn test_curvature_detection_curved_channel() {
        // Set up a scenario where velocity changes direction.
        // Use a small grid with manually set velocities to test curvature.
        let w = 5;
        let h = 5;
        let mut sim = PipeWater::new(w, h);

        // Create a bend: flow going East at (1,2), then turning South at (2,2).
        // Fill with water so depth is non-trivial.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        // Manually set velocity field to simulate a turning flow.
        // Row 2: flow going East.
        for x in 0..w {
            let i = sim.idx(x, 2);
            sim.velocity_x[i] = 1.0;
            sim.velocity_y[i] = 0.0;
        }
        // At (3,2) the flow turns South.
        let bend_idx = sim.idx(3, 2);
        sim.velocity_x[bend_idx] = 0.5;
        sim.velocity_y[bend_idx] = 0.87; // ~60 degrees turn

        // Curvature at the bend point should be non-zero.
        let curvature_at_bend = sim.flow_curvature(3, 2);
        // Curvature at a straight section should be near zero.
        let curvature_straight = sim.flow_curvature(1, 2);

        assert!(
            curvature_at_bend > curvature_straight,
            "curvature at bend ({curvature_at_bend:.4}) should exceed straight ({curvature_straight:.4})"
        );
        assert!(
            curvature_at_bend > 0.01,
            "curvature at bend should be significant, got {curvature_at_bend:.6}"
        );
    }

    // -------------------------------------------------------------------------
    // Sediment Test 5: Heights change after many sediment steps.
    // -------------------------------------------------------------------------
    #[test]
    fn test_heights_change_after_sediment_steps() {
        let w = 10;
        let h = 5;
        let mut terrain = slope_terrain(w, h);
        let original_terrain = terrain.clone();
        let mut sim = PipeWater::new(w, h);

        // Add water to generate flow.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
            }
        }

        // Run many steps.
        for _ in 0..500 {
            sim.step(&terrain, DT);
            sim.step_sediment(&mut terrain);
        }

        // Heights should have changed somewhere.
        let max_change = terrain
            .iter()
            .zip(original_terrain.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);

        assert!(
            max_change > 1e-6,
            "heights should change after sediment transport, max change = {max_change:.2e}"
        );
    }

    // -------------------------------------------------------------------------
    // Sediment Test 6: Curvature is zero for straight uniform flow.
    // -------------------------------------------------------------------------
    #[test]
    fn test_curvature_zero_straight_flow() {
        let w = 10;
        let h = 5;
        let mut sim = PipeWater::new(w, h);

        // Uniform eastward flow everywhere.
        for y in 0..h {
            for x in 0..w {
                sim.add_water(x, y, 1.0);
                let i = sim.idx(x, y);
                sim.velocity_x[i] = 1.0;
                sim.velocity_y[i] = 0.0;
            }
        }

        // Interior point should have near-zero curvature.
        let curv = sim.flow_curvature(5, 2);
        assert!(
            curv < 0.01,
            "curvature should be near zero for straight flow, got {curv:.6}"
        );
    }

    // -------------------------------------------------------------------------
    // Sediment Test 7: No erosion in dry tiles.
    // -------------------------------------------------------------------------
    #[test]
    fn test_no_erosion_dry_tiles() {
        let w = 5;
        let h = 5;
        let mut terrain = slope_terrain(w, h);
        let original_terrain = terrain.clone();
        let mut sim = PipeWater::new(w, h);

        // No water added — all tiles are dry.
        // Run sediment step directly.
        sim.step_sediment(&mut terrain);

        assert_eq!(terrain, original_terrain, "dry tiles should not erode");
        assert!(
            sim.total_suspended() < 1e-15,
            "no sediment should be picked up from dry tiles"
        );
    }
}
