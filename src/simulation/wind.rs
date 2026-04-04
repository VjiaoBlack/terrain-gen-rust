use super::day_night::Season;

/// 2D wind vector field shaped by terrain. Wind flows around mountains,
/// funnels through passes, creates rain shadows on leeward sides.
/// Computed once per wind direction change (~seasonal), with curl noise
/// turbulence added each tick for variation.
pub struct WindField {
    pub width: usize,
    pub height: usize,
    /// Per-tile x component of wind vector.
    pub wind_x: Vec<f64>,
    /// Per-tile y component of wind vector.
    pub wind_y: Vec<f64>,
    /// Cached magnitude of (wind_x, wind_y) per tile.
    pub wind_speed: Vec<f64>,
    /// Wind shadow factor: 0.0 = full shadow (behind tall mountain),
    /// 1.0 = fully exposed to wind.
    pub wind_shadow: Vec<f64>,
    /// Atmospheric moisture carried by the wind at each tile.
    /// Increases over water bodies, decreases via orographic lift (rain).
    /// Separate from tile moisture — this is moisture in the air column.
    pub moisture_carried: Vec<f64>,
    /// Prevailing wind direction in radians (0 = east, PI/2 = north).
    pub prevailing_dir: f64,
    /// Prevailing wind strength (0.0-1.0).
    pub prevailing_strength: f64,
}

impl WindField {
    /// Create a new empty wind field.
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            wind_x: vec![0.0; n],
            wind_y: vec![0.0; n],
            wind_speed: vec![0.0; n],
            wind_shadow: vec![1.0; n],
            moisture_carried: vec![0.0; n],
            prevailing_dir: std::f64::consts::PI, // default: westerly (blowing east-to-west)
            prevailing_strength: 0.6,
        }
    }

    /// Compute the full wind field from terrain heights using a Jos Stam
    /// "Stable Fluids" incompressible Navier-Stokes solver.
    ///
    /// Algorithm (per iteration):
    /// 1. Add forces — push toward prevailing direction
    /// 2. Diffuse — small viscosity smoothing via Gauss-Seidel
    /// 3. Project — pressure solve to enforce incompressibility
    /// 4. Advect — semi-Lagrangian self-advection with bilinear interpolation
    /// 5. Project again — re-enforce divergence-free constraint
    ///
    /// Terrain height acts as a pressure source — high terrain pushes wind
    /// sideways and over rather than blocking it completely.  Height-based
    /// damping slows wind over mountains without zeroing it.
    /// Chokepoints naturally emerge from the fluid dynamics around terrain.
    /// The solver runs 30 iterations to reach approximate steady state.
    pub fn compute_from_terrain(
        heights: &[f64],
        width: usize,
        height: usize,
        prevailing_dir: f64,
        prevailing_strength: f64,
        chokepoint_scores: Option<&[f64]>,
    ) -> Self {
        let n = width * height;
        let mut field = Self {
            width,
            height,
            wind_x: vec![0.0; n],
            wind_y: vec![0.0; n],
            wind_speed: vec![0.0; n],
            wind_shadow: vec![1.0; n],
            moisture_carried: vec![0.0; n],
            prevailing_dir,
            prevailing_strength,
        };

        if n == 0 {
            return field;
        }

        let base_wx = prevailing_dir.cos() * prevailing_strength;
        let base_wy = prevailing_dir.sin() * prevailing_strength;

        // Initialize velocity field with prevailing wind everywhere
        let mut vx = vec![0.0f64; n];
        let mut vy = vec![0.0f64; n];
        for i in 0..n {
            vx[i] = base_wx;
            vy[i] = base_wy;
        }

        // Run Stam solver iterations to reach steady state.
        // Each iteration: relax toward prevailing -> diffuse -> project -> advect -> project.
        // Terrain height gradient injects pressure in the projection step,
        // deflecting wind around and over mountains.  Height-based damping
        // slows wind on high terrain without fully blocking it.
        let viscosity = 0.0001;
        let dt = 1.0;
        let terrain_pressure_strength = 0.8;
        let relax_rate = 0.05; // how fast cells return toward prevailing wind
        for _ in 0..50 {
            // Step 1: Relax toward prevailing wind (gentle drag)
            for i in 0..n {
                vx[i] += relax_rate * (base_wx - vx[i]);
                vy[i] += relax_rate * (base_wy - vy[i]);
            }
            // Step 2: Diffuse
            stam_diffuse(&mut vx, width, height, viscosity);
            stam_diffuse(&mut vy, width, height, viscosity);
            // Step 3: Project (make divergence-free, terrain height as pressure source)
            stam_project(
                &mut vx,
                &mut vy,
                width,
                height,
                heights,
                terrain_pressure_strength,
            );
            // Step 4: Advect (semi-Lagrangian self-advection)
            let old_vx = vx.clone();
            let old_vy = vy.clone();
            stam_advect(&mut vx, &old_vx, &old_vy, width, height, dt);
            stam_advect(&mut vy, &old_vx, &old_vy, width, height, dt);
            // Step 5: Project again
            stam_project(
                &mut vx,
                &mut vy,
                width,
                height,
                heights,
                terrain_pressure_strength,
            );
            // Height-based damping: high terrain = more drag, but never fully blocked
            for i in 0..n {
                let drag = (heights[i] - 0.5).max(0.0) * 1.2;
                let damping = 1.0 / (1.0 + drag);
                vx[i] *= damping;
                vy[i] *= damping;
            }
        }

        // Apply chokepoint boost if provided
        if let Some(scores) = chokepoint_scores {
            const CHOKEPOINT_BOOST: f64 = 2.5;
            for i in 0..n {
                let score = scores[i];
                if score > 0.05 {
                    let boost = 1.0 + score * CHOKEPOINT_BOOST;
                    vx[i] *= boost;
                    vy[i] *= boost;
                }
            }
        }

        // Build output field
        for i in 0..n {
            field.wind_x[i] = vx[i];
            field.wind_y[i] = vy[i];
            field.wind_speed[i] = (vx[i] * vx[i] + vy[i] * vy[i]).sqrt();
        }

        // Compute wind shadow by combining two factors:
        // 1. Speed ratio: how fast wind is here vs prevailing (from fluid solve)
        // 2. Upwind obstruction: trace backward along prevailing direction and
        //    check if tall terrain blocks the path (geometric shadow).
        // The minimum of the two gives a robust shadow estimate.
        if prevailing_strength > 0.0 {
            let prev_dx = prevailing_dir.cos();
            let prev_dy = prevailing_dir.sin();
            for y in 0..height {
                for x in 0..width {
                    let i = y * width + x;
                    let speed_shadow = (field.wind_speed[i] / prevailing_strength).min(1.0);

                    // Trace upwind up to 12 tiles. If any tile has significantly
                    // taller terrain, reduce shadow.
                    let h_here = if i < heights.len() { heights[i] } else { 0.0 };
                    let mut geo_shadow = 1.0f64;
                    for step in 1..=12 {
                        let sx = (x as f64 - prev_dx * step as f64).round() as i32;
                        let sy = (y as f64 - prev_dy * step as f64).round() as i32;
                        if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                            let si = sy as usize * width + sx as usize;
                            let h_up = heights[si];
                            let elevation_diff = h_up - h_here;
                            if elevation_diff > 0.1 {
                                // Taller terrain upwind casts shadow, decaying with distance
                                let block = (elevation_diff * 2.0 / step as f64).min(1.0);
                                geo_shadow = geo_shadow.min(1.0 - block);
                            }
                        }
                    }
                    field.wind_shadow[i] = speed_shadow.min(geo_shadow.max(0.0));
                }
            }
        }

        field
    }

    /// Add curl noise turbulence to the wind field. This modifies the
    /// cached field slightly, creating natural variation without changing
    /// the mean direction significantly.
    ///
    /// Uses Perlin noise sampled at each tile position, scaled by time
    /// so the turbulence evolves slowly.
    pub fn add_curl_noise(&mut self, time: f64, seed: u32) {
        use noise::{NoiseFn, Perlin};

        let perlin = Perlin::new(seed);
        let turbulence_strength = 0.08 * self.prevailing_strength;

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * self.width + x;
                let nx = x as f64 * 0.05;
                let ny = y as f64 * 0.05;
                let nt = time * 0.01;

                let turbulence_x = perlin.get([nx, ny, nt]) * turbulence_strength;
                let turbulence_y = perlin.get([nx + 100.0, ny + 100.0, nt]) * turbulence_strength;

                self.wind_x[idx] += turbulence_x;
                self.wind_y[idx] += turbulence_y;
                self.wind_speed[idx] = (self.wind_x[idx] * self.wind_x[idx]
                    + self.wind_y[idx] * self.wind_y[idx])
                    .sqrt();
            }
        }
    }

    /// Return wind vector at position (x, y). Returns (0, 0) for out-of-bounds.
    pub fn get_wind(&self, x: usize, y: usize) -> (f64, f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            (self.wind_x[idx], self.wind_y[idx])
        } else {
            (0.0, 0.0)
        }
    }

    /// Return wind speed at position (x, y). Returns 0.0 for out-of-bounds.
    pub fn get_speed(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.wind_speed[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Return wind shadow factor at position (x, y). Returns 1.0 for out-of-bounds.
    pub fn get_shadow(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.wind_shadow[y * self.width + x]
        } else {
            1.0
        }
    }

    /// Return atmospheric moisture carried by the wind at (x, y).
    pub fn get_moisture_carried(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.moisture_carried[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Advect atmospheric moisture through the wind field for one step.
    ///
    /// - Over water tiles (`is_water` callback returns true), moisture is picked up.
    /// - When wind pushes air uphill (orographic lift), moisture precipitates as rain.
    /// - Otherwise moisture is transported downwind via semi-Lagrangian advection.
    ///
    /// Returns a Vec of orographic precipitation amounts per tile (rain deposited).
    /// Returns (precipitation, evaporated) — both per-tile Vec<f64>.
    /// Caller must subtract evaporated from surface water to conserve mass.
    pub fn advect_moisture(
        &mut self,
        heights: &[f64],
        ocean_mask: &[bool],
        soil_moisture: &[f64],
    ) -> (Vec<f64>, Vec<f64>) {
        let w = self.width;
        let h = self.height;
        let n = w * h;
        let mut precip = vec![0.0f64; n];
        let mut evaporated = vec![0.0f64; n];

        // Phase 1: Evaporation (unified hydrology Step 2)
        // Ocean tiles load moisture proportional to wind speed.
        // Land tiles with soil moisture evapotranspire at a lower rate.
        const OCEAN_EVAP_RATE: f64 = 0.005;
        const LAND_EVAPO_RATE: f64 = 0.001;
        for i in 0..n {
            if ocean_mask[i] {
                let pickup = OCEAN_EVAP_RATE * self.wind_speed[i];
                self.moisture_carried[i] += pickup;
                evaporated[i] = pickup;
            } else {
                // Evapotranspiration from soil moisture
                let evapo = LAND_EVAPO_RATE * soil_moisture[i];
                self.moisture_carried[i] += evapo;
                evaporated[i] = evapo;
            }
        }

        // Phase 2: Orographic precipitation — wind pushing air uphill drops rain
        // Low rate so only significant slopes cause rain (mountains, not gentle hills)
        const OROGRAPHIC_PRECIP_RATE: f64 = 0.05;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let (wx, wy) = (self.wind_x[i], self.wind_y[i]);
                let h_here = heights[i];
                let h_left = if x > 0 { heights[i - 1] } else { h_here };
                let h_right = if x + 1 < w { heights[i + 1] } else { h_here };
                let h_up = if y > 0 { heights[i - w] } else { h_here };
                let h_down = if y + 1 < h { heights[i + w] } else { h_here };
                let slope_x = (h_right - h_left) * 0.5;
                let slope_y = (h_down - h_up) * 0.5;

                let lift = (wx * slope_x + wy * slope_y).max(0.0);
                let rain = self.moisture_carried[i] * lift * OROGRAPHIC_PRECIP_RATE;
                self.moisture_carried[i] -= rain;
                precip[i] += rain;
            }
        }

        // Phase 3: Semi-Lagrangian advection of moisture_carried
        // Multiply wind by transport_speed so moisture crosses the map in reasonable time.
        // At speed 5.0, wind of 0.6 moves moisture 3 tiles per call (called every 3 ticks).
        const TRANSPORT_SPEED: f64 = 5.0;
        let old = self.moisture_carried.clone();
        let wf = w as f64;
        let hf = h as f64;
        if w >= 3 && h >= 3 {
            for y in 0..h {
                for x in 0..w {
                    let idx = y * w + x;
                    let px = (x as f64) - self.wind_x[idx] * TRANSPORT_SPEED;
                    let py = (y as f64) - self.wind_y[idx] * TRANSPORT_SPEED;
                    let px = px.clamp(0.5, wf - 1.5);
                    let py = py.clamp(0.5, hf - 1.5);
                    let i0 = px.floor() as usize;
                    let j0 = py.floor() as usize;
                    let i1 = i0 + 1;
                    let j1 = j0 + 1;
                    let sx = px - i0 as f64;
                    let sy = py - j0 as f64;
                    self.moisture_carried[idx] = (1.0 - sx) * (1.0 - sy) * old[j0 * w + i0]
                        + sx * (1.0 - sy) * old[j0 * w + i1]
                        + (1.0 - sx) * sy * old[j1 * w + i0]
                        + sx * sy * old[j1 * w + i1];
                }
            }
        }

        // Clamp
        for v in self.moisture_carried.iter_mut() {
            *v = v.clamp(0.0, 1.0);
        }

        (precip, evaporated)
    }

    /// Get the prevailing wind direction for a given season.
    /// Returns direction in radians: 0 = east, PI/2 = north.
    pub fn seasonal_direction(season: Season) -> f64 {
        match season {
            // Westerly winds in spring/autumn (wind blows FROM west, i.e. toward east)
            Season::Spring => 0.0, // east (wind blows eastward)
            Season::Summer => 0.3, // slightly NE (variable summer winds)
            Season::Autumn => 0.0, // east (westerly)
            Season::Winter => -std::f64::consts::FRAC_PI_4, // SE (northerly component)
        }
    }
}

// ---------------------------------------------------------------------------
// Jos Stam "Stable Fluids" helper functions
// All operate on flat Vec<f64> indexed as [y * width + x].
// ---------------------------------------------------------------------------

/// Gauss-Seidel diffusion: implicitly diffuse `field` with given viscosity.
/// Solves (I - viscosity * Laplacian) * new = old via 20 Gauss-Seidel iterations.
fn stam_diffuse(field: &mut [f64], w: usize, h: usize, viscosity: f64) {
    let old = field.to_vec();
    let a = viscosity; // dt=1 absorbed
    let denom = 1.0 + 4.0 * a;
    for _ in 0..20 {
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let left = if x > 0 { field[idx - 1] } else { field[idx] };
                let right = if x + 1 < w {
                    field[idx + 1]
                } else {
                    field[idx]
                };
                let down = if y > 0 { field[idx - w] } else { field[idx] };
                let up = if y + 1 < h {
                    field[idx + w]
                } else {
                    field[idx]
                };
                field[idx] = (old[idx] + a * (left + right + down + up)) / denom;
            }
        }
    }
}

/// Pressure projection: make the velocity field divergence-free.
/// Solves the pressure Poisson equation via 20 Gauss-Seidel iterations,
/// then subtracts the pressure gradient from velocity.
/// Terrain height gradients are injected as a pressure source so that
/// high terrain pushes wind away rather than acting as a solid wall.
fn stam_project(
    vx: &mut [f64],
    vy: &mut [f64],
    w: usize,
    h: usize,
    heights: &[f64],
    terrain_pressure_strength: f64,
) {
    let n = w * h;
    let mut div = vec![0.0f64; n];
    let mut p = vec![0.0f64; n];

    // Compute divergence with terrain height gradient as pressure source
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let vx_right = if x + 1 < w { vx[idx + 1] } else { vx[idx] };
            let vx_left = if x > 0 { vx[idx - 1] } else { vx[idx] };
            let vy_up = if y + 1 < h { vy[idx + w] } else { vy[idx] };
            let vy_down = if y > 0 { vy[idx - w] } else { vy[idx] };

            // Terrain height gradient creates pressure — high terrain pushes wind away
            let h_right = if x + 1 < w {
                heights[idx + 1]
            } else {
                heights[idx]
            };
            let h_left = if x > 0 {
                heights[idx - 1]
            } else {
                heights[idx]
            };
            let h_up = if y + 1 < h {
                heights[idx + w]
            } else {
                heights[idx]
            };
            let h_down = if y > 0 {
                heights[idx - w]
            } else {
                heights[idx]
            };
            let terrain_div = terrain_pressure_strength * (h_right - h_left + h_up - h_down);

            div[idx] = -0.5 * (vx_right - vx_left + vy_up - vy_down) + terrain_div;
        }
    }

    // Solve pressure Poisson equation: Laplacian(p) = div
    // Dirichlet boundary: p = 0 at map edges
    for _ in 0..40 {
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                // Dirichlet: pressure = 0 at boundaries
                if x == 0 || x == w - 1 || y == 0 || y == h - 1 {
                    p[idx] = 0.0;
                    continue;
                }
                let left = p[idx - 1];
                let right = p[idx + 1];
                let down = p[idx - w];
                let up = p[idx + w];
                p[idx] = (div[idx] + left + right + down + up) / 4.0;
            }
        }
    }

    // Subtract pressure gradient from velocity
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let p_right = if x + 1 < w { p[idx + 1] } else { p[idx] };
            let p_left = if x > 0 { p[idx - 1] } else { p[idx] };
            let p_up = if y + 1 < h { p[idx + w] } else { p[idx] };
            let p_down = if y > 0 { p[idx - w] } else { p[idx] };
            vx[idx] -= 0.5 * (p_right - p_left);
            vy[idx] -= 0.5 * (p_up - p_down);
        }
    }
}

/// Semi-Lagrangian advection: trace each cell backward through the velocity
/// field and sample the old value with bilinear interpolation.
fn stam_advect(field: &mut [f64], vx: &[f64], vy: &[f64], w: usize, h: usize, dt: f64) {
    let old = field.to_vec();
    let wf = w as f64;
    let hf = h as f64;

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            // Trace backward
            let px = (x as f64) - dt * vx[idx];
            let py = (y as f64) - dt * vy[idx];
            // Clamp to grid bounds
            let px = px.clamp(0.5, wf - 1.5);
            let py = py.clamp(0.5, hf - 1.5);
            // Bilinear interpolation
            let i0 = px.floor() as usize;
            let j0 = py.floor() as usize;
            let i1 = i0 + 1;
            let j1 = j0 + 1;
            let sx = px - i0 as f64;
            let sy = py - j0 as f64;
            field[idx] = (1.0 - sx) * (1.0 - sy) * old[j0 * w + i0]
                + sx * (1.0 - sy) * old[j0 * w + i1]
                + (1.0 - sx) * sy * old[j1 * w + i0]
                + sx * sy * old[j1 * w + i1];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_flat_terrain_passes_at_prevailing_speed() {
        // Flat terrain: wind should pass through at roughly prevailing speed everywhere
        let w = 64;
        let h = 64;
        let heights = vec![0.5; w * h];
        let dir = 0.0; // eastward
        let strength = 0.8;
        let field = WindField::compute_from_terrain(&heights, w, h, dir, strength, None);

        // Check center tiles — speed should be close to prevailing strength
        for y in 10..54 {
            for x in 10..54 {
                let speed = field.get_speed(x, y);
                assert!(
                    (speed - strength).abs() < 0.05,
                    "flat terrain at ({},{}) speed {} should be ~{}",
                    x,
                    y,
                    speed,
                    strength
                );
                let shadow = field.get_shadow(x, y);
                assert!(
                    (shadow - 1.0).abs() < 0.01,
                    "flat terrain should have no wind shadow"
                );
            }
        }
    }

    #[test]
    fn wind_mountain_blocks_leeward() {
        // Mountain ridge across the middle, wind blowing eastward (dir=0).
        // Wind shadow behind the ridge: leeward side should have reduced
        // wind shadow factor compared to far upwind.
        let w = 64;
        let h = 64;
        let mut heights = vec![0.1; w * h];

        // Place mountain ridge at x=30, spanning full y range
        for y in 0..h {
            for dx in 0..3 {
                heights[y * w + (29 + dx)] = 0.9;
            }
        }

        let field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        // Leeward shadow (x=35) should be reduced compared to far upwind (x=10)
        let upwind_shadow = field.get_shadow(10, 32);
        let leeward_shadow = field.get_shadow(35, 32);
        assert!(
            leeward_shadow < upwind_shadow,
            "leeward shadow {} should be less than upwind shadow {}",
            leeward_shadow,
            upwind_shadow
        );
    }

    #[test]
    fn wind_shadow_reduced_behind_tall_mountain() {
        // A very tall mountain should create reduced but non-zero wind on leeward side.
        // With the pressure-based solver, wind is deflected and slowed, not blocked.
        let w = 64;
        let h = 64;
        let mut heights = vec![0.0; w * h];

        // Tall mountain wall at x=30
        for y in 10..54 {
            for dx in 0..5 {
                heights[y * w + (28 + dx)] = 2.0; // very tall
            }
        }

        let field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        // Shadow behind the wall is noticeably less than 1.0 (wind slowed)
        let shadow = field.get_shadow(35, 32);
        assert!(
            shadow < 0.7,
            "shadow behind very tall mountain should be reduced, got {}",
            shadow
        );

        // But speed on the mountain itself is NOT zero — pressure-based solver
        // allows some flow over/through high terrain
        let mountain_speed = field.get_speed(30, 32);
        assert!(
            mountain_speed > 0.0,
            "mountain top speed should be non-zero with pressure solver, got {}",
            mountain_speed
        );
    }

    #[test]
    fn wind_speed_on_mountain_lower_than_flat_but_not_zero() {
        // Core property of the pressure-based solver: mountains slow wind
        // but don't block it entirely.
        let w = 64;
        let h = 64;

        // Flat terrain baseline
        let flat_heights = vec![0.3; w * h];
        let flat_field = WindField::compute_from_terrain(&flat_heights, w, h, 0.0, 0.8, None);
        let flat_speed = flat_field.get_speed(32, 32);

        // Mountain terrain
        let mut mt_heights = vec![0.3; w * h];
        for y in 20..44 {
            for x in 20..44 {
                mt_heights[y * w + x] = 0.9;
            }
        }
        let mt_field = WindField::compute_from_terrain(&mt_heights, w, h, 0.0, 0.8, None);
        let mt_speed = mt_field.get_speed(32, 32);

        assert!(
            mt_speed < flat_speed,
            "mountain speed {} should be less than flat speed {}",
            mt_speed,
            flat_speed
        );
        assert!(
            mt_speed > 0.0,
            "mountain speed should be non-zero, got {}",
            mt_speed
        );
    }

    #[test]
    fn wind_valley_funnels_boost() {
        // Create a narrow gap in a mountain range — wind should be boosted
        let w = 64;
        let h = 64;
        let mut heights = vec![0.1; w * h];

        // Mountain range with a 3-tile gap at y=32
        for y in 0..h {
            if !(31..=33).contains(&y) {
                // Mountains everywhere except the gap
                for dx in 0..3 {
                    heights[y * w + (30 + dx)] = 0.9;
                }
            }
        }

        // Create chokepoint scores: high score at the gap
        let mut choke_scores = vec![0.0; w * h];
        for y in 31..=33 {
            for x in 30..33 {
                choke_scores[y * w + x] = 0.5; // narrow passage
            }
        }

        let field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, Some(&choke_scores));

        // Speed at the gap should be higher than on flat terrain without chokepoint
        let flat_field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);
        let gap_speed = field.get_speed(31, 32);
        let flat_gap_speed = flat_field.get_speed(31, 32);
        assert!(
            gap_speed > flat_gap_speed,
            "gap speed with chokepoint {} should exceed without {}",
            gap_speed,
            flat_gap_speed
        );
    }

    #[test]
    fn wind_curl_noise_preserves_mean_direction() {
        // Curl noise should not change the mean wind direction significantly
        let w = 32;
        let h = 32;
        let heights = vec![0.3; w * h];
        let mut field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        // Record mean direction before noise
        let n = (w * h) as f64;
        let mean_x_before: f64 = field.wind_x.iter().sum::<f64>() / n;
        let mean_y_before: f64 = field.wind_y.iter().sum::<f64>() / n;

        field.add_curl_noise(100.0, 42);

        let mean_x_after: f64 = field.wind_x.iter().sum::<f64>() / n;
        let mean_y_after: f64 = field.wind_y.iter().sum::<f64>() / n;

        // Mean direction should be within ~20% of original
        assert!(
            (mean_x_after - mean_x_before).abs() < 0.1,
            "curl noise shifted mean x from {} to {}",
            mean_x_before,
            mean_x_after
        );
        assert!(
            (mean_y_after - mean_y_before).abs() < 0.1,
            "curl noise shifted mean y from {} to {}",
            mean_y_before,
            mean_y_after
        );
    }

    #[test]
    fn wind_at_map_edges_no_crash() {
        let w = 16;
        let h = 16;
        let heights = vec![0.5; w * h];
        let field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

        // Corners
        let _ = field.get_wind(0, 0);
        let _ = field.get_wind(w - 1, 0);
        let _ = field.get_wind(0, h - 1);
        let _ = field.get_wind(w - 1, h - 1);

        // Out of bounds
        assert_eq!(field.get_wind(w + 10, h + 10), (0.0, 0.0));
        assert_eq!(field.get_speed(w + 10, h + 10), 0.0);
        assert_eq!(field.get_shadow(w + 10, h + 10), 1.0);

        // Edge tiles should have valid (non-NaN) values
        for x in 0..w {
            let s = field.get_speed(x, 0);
            assert!(!s.is_nan(), "speed at ({},0) is NaN", x);
            let s = field.get_speed(x, h - 1);
            assert!(!s.is_nan(), "speed at ({},{}) is NaN", x, h - 1);
        }
    }

    #[test]
    fn wind_seasonal_direction_rotates() {
        let spring_dir = WindField::seasonal_direction(Season::Spring);
        let summer_dir = WindField::seasonal_direction(Season::Summer);
        let winter_dir = WindField::seasonal_direction(Season::Winter);

        // Different seasons should have different wind directions
        assert!(
            (spring_dir - summer_dir).abs() > 0.01,
            "spring and summer should have different wind directions"
        );
        assert!(
            (spring_dir - winter_dir).abs() > 0.01,
            "spring and winter should have different wind directions"
        );
        assert!(
            (summer_dir - winter_dir).abs() > 0.01,
            "summer and winter should have different wind directions"
        );
    }

    #[test]
    fn wind_seasonal_recompute_changes_field() {
        // Recomputing with a different direction should produce a different field
        let w = 32;
        let h = 32;
        let mut heights = vec![0.3; w * h];
        // Add some terrain variation to make direction matter
        for y in 10..20 {
            for x in 10..20 {
                heights[y * w + x] = 0.8;
            }
        }

        let spring_dir = WindField::seasonal_direction(Season::Spring);
        let winter_dir = WindField::seasonal_direction(Season::Winter);

        let field_spring = WindField::compute_from_terrain(&heights, w, h, spring_dir, 0.7, None);
        let field_winter = WindField::compute_from_terrain(&heights, w, h, winter_dir, 0.7, None);

        // Fields should differ at some tiles
        let mut any_diff = false;
        for i in 0..(w * h) {
            if (field_spring.wind_x[i] - field_winter.wind_x[i]).abs() > 0.01
                || (field_spring.wind_y[i] - field_winter.wind_y[i]).abs() > 0.01
            {
                any_diff = true;
                break;
            }
        }
        assert!(
            any_diff,
            "seasonal direction change should produce a different wind field"
        );
    }

    #[test]
    fn wind_empty_map_no_panic() {
        // Zero-size map should not panic
        let field = WindField::compute_from_terrain(&[], 0, 0, 0.0, 0.5, None);
        assert_eq!(field.width, 0);
        assert_eq!(field.height, 0);
        assert_eq!(field.get_wind(0, 0), (0.0, 0.0));
    }

    #[test]
    fn wind_new_default_values() {
        let field = WindField::new(10, 10);
        assert_eq!(field.width, 10);
        assert_eq!(field.height, 10);
        assert_eq!(field.wind_x.len(), 100);
        assert_eq!(field.wind_shadow.len(), 100);
        // Default shadow should be 1.0 (fully exposed)
        for &s in &field.wind_shadow {
            assert_eq!(s, 1.0);
        }
    }

    // -----------------------------------------------------------------------
    // Issue 1 diagnostic tests: wind deflection around mountains
    // -----------------------------------------------------------------------

    #[test]
    fn wind_deflects_around_central_mountain() {
        // 64x64 map, flat at 0.1 except a 10x10 mountain block (height 0.9) in the center.
        // Wind blows from the west (dir=0.0 => eastward).
        // Tiles north and south of the mountain should have a deflected y-component.
        let w = 64;
        let h = 64;
        let mut heights = vec![0.1f64; w * h];

        // Place 10x10 mountain at center (27..37, 27..37)
        for y in 27..37 {
            for x in 27..37 {
                heights[y * w + x] = 0.9;
            }
        }

        let dir = 0.0; // east (wind blows from west to east)
        let strength = 0.8;
        let field = WindField::compute_from_terrain(&heights, w, h, dir, strength, None);

        // Sample tiles just north of the mountain (y=24, along the mountain's x range)
        // Wind should have a negative y-component (deflected northward, away from mountain)
        let mut north_wy_sum = 0.0;
        let mut north_count = 0;
        for x in 28..36 {
            let (_, wy) = field.get_wind(x, 24);
            north_wy_sum += wy;
            north_count += 1;
        }
        let north_wy_avg = north_wy_sum / north_count as f64;

        // Sample tiles just south of the mountain (y=39)
        // Wind should have a positive y-component (deflected southward)
        let mut south_wy_sum = 0.0;
        let mut south_count = 0;
        for x in 28..36 {
            let (_, wy) = field.get_wind(x, 39);
            south_wy_sum += wy;
            south_count += 1;
        }
        let south_wy_avg = south_wy_sum / south_count as f64;

        eprintln!("=== Wind Deflection Diagnostic ===");
        eprintln!("North of mountain (y=24) avg wy: {:.4}", north_wy_avg);
        eprintln!("South of mountain (y=39) avg wy: {:.4}", south_wy_avg);

        // Print a few sample points on and around the mountain
        for &(label, x, y) in &[
            ("Upwind (20,32)", 20, 32),
            ("North edge (32,25)", 32, 25),
            ("On mountain (32,32)", 32, 32),
            ("South edge (32,38)", 32, 38),
            ("Downwind (44,32)", 44, 32),
        ] {
            let (wx, wy) = field.get_wind(x, y);
            let spd = field.get_speed(x, y);
            eprintln!("  {}: wx={:.4}, wy={:.4}, speed={:.4}", label, wx, wy, spd);
        }

        // The key assertion: wind north and south of the mountain should
        // deflect in OPPOSITE y-directions (north gets pushed north, south pushed south).
        // This means north_wy and south_wy should have opposite signs, or at minimum
        // north_wy < south_wy (north deflects more northward / less southward).
        assert!(
            north_wy_avg < south_wy_avg - 0.01,
            "Wind should deflect away from mountain: north wy ({:.4}) should be less than south wy ({:.4})",
            north_wy_avg,
            south_wy_avg
        );
    }

    #[test]
    fn wind_reduced_on_mountain_but_not_zero() {
        // Wind on a mountain should be slower than on a fully flat map at the
        // same position, but not zero. We compare against a flat baseline to
        // avoid boundary effects.
        let w = 64;
        let h = 64;

        // Flat baseline
        let flat_heights = vec![0.1f64; w * h];
        let flat_field = WindField::compute_from_terrain(&flat_heights, w, h, 0.0, 0.8, None);
        let flat_speed = flat_field.get_speed(32, 32);

        // Mountain map
        let mut mt_heights = vec![0.1f64; w * h];
        for y in 27..37 {
            for x in 27..37 {
                mt_heights[y * w + x] = 0.9;
            }
        }
        let mt_field = WindField::compute_from_terrain(&mt_heights, w, h, 0.0, 0.8, None);
        let mountain_speed = mt_field.get_speed(32, 32);

        eprintln!("=== Mountain Speed Diagnostic ===");
        eprintln!("Flat baseline speed at (32,32): {:.4}", flat_speed);
        eprintln!("Mountain speed at (32,32): {:.4}", mountain_speed);

        assert!(
            mountain_speed > 0.01,
            "Wind on mountain should not be zero, got {:.4}",
            mountain_speed
        );
        assert!(
            mountain_speed < flat_speed,
            "Wind on mountain ({:.4}) should be slower than flat baseline ({:.4})",
            mountain_speed,
            flat_speed
        );
    }

    #[test]
    fn wind_funnel_between_two_mountains() {
        // Two mountains with a gap between them. Wind through the gap should
        // be faster than wind on open flat terrain (Venturi / funnel effect).
        let w = 64;
        let h = 64;
        let mut heights = vec![0.1f64; w * h];

        // Mountain A: rows 10..22, cols 25..39 (north mountain)
        for y in 10..22 {
            for x in 25..39 {
                heights[y * w + x] = 0.9;
            }
        }
        // Mountain B: rows 42..54, cols 25..39 (south mountain)
        for y in 42..54 {
            for x in 25..39 {
                heights[y * w + x] = 0.9;
            }
        }
        // Gap is rows 22..42 (20 tiles wide) between the two mountains

        let field = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        // Speed in the gap (center of gap, middle of mountain x range)
        let gap_speed = field.get_speed(32, 32);
        // Speed on open flat terrain far from mountains
        let open_speed = field.get_speed(5, 5);

        eprintln!("=== Funnel Effect Diagnostic ===");
        eprintln!("Gap speed (32,32): {:.4}", gap_speed);
        eprintln!("Open terrain speed (5,5): {:.4}", open_speed);

        // The gap should funnel wind to be faster than open terrain
        assert!(
            gap_speed > open_speed * 1.05,
            "Wind in gap ({:.4}) should be faster than open terrain ({:.4})",
            gap_speed,
            open_speed
        );
    }

    // -----------------------------------------------------------------------
    // Issue 2 diagnostic tests: wind carrying moisture
    // -----------------------------------------------------------------------

    #[test]
    fn atmospheric_moisture_carried_over_water() {
        // Wind blowing east over a water body should pick up atmospheric moisture.
        // Downwind tiles should have moisture_carried > 0.
        let w = 40;
        let h = 20;
        let heights = vec![0.1f64; w * h];
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        // Water body at columns 5..15 (treated as ocean)
        let n = w * h;
        let mut ocean_mask = vec![false; n];
        for y in 0..h {
            for x in 5..15 {
                ocean_mask[y * w + x] = true;
            }
        }
        let soil_moisture = vec![0.0f64; n];

        // Run several advection steps (100 ticks for the lower unified hydrology evap rate)
        for _ in 0..100 {
            let (_precip, _evap) = wind.advect_moisture(&heights, &ocean_mask, &soil_moisture);
        }

        // Downwind of water (x=20), should have atmospheric moisture
        let mut downwind_moisture = 0.0;
        for y in 5..15 {
            downwind_moisture += wind.get_moisture_carried(20, y);
        }
        let downwind_avg = downwind_moisture / 10.0;

        // Upwind of water (x=2), should have nearly none
        let mut upwind_moisture = 0.0;
        for y in 5..15 {
            upwind_moisture += wind.get_moisture_carried(2, y);
        }
        let upwind_avg = upwind_moisture / 10.0;

        eprintln!("=== Atmospheric Moisture Diagnostic ===");
        eprintln!("Downwind avg moisture_carried (x=20): {:.4}", downwind_avg);
        eprintln!("Upwind avg moisture_carried (x=2): {:.4}", upwind_avg);

        assert!(
            downwind_avg > 0.005,
            "Downwind of water should carry moisture, got {:.4}",
            downwind_avg
        );
        assert!(
            downwind_avg > upwind_avg * 2.0,
            "Downwind ({:.4}) should have much more moisture than upwind ({:.4})",
            downwind_avg,
            upwind_avg
        );
    }

    #[test]
    fn atmospheric_moisture_orographic_rain() {
        // Wind blows east, water body on the west, mountain in the middle.
        // Orographic lift should cause precipitation on the windward side of the mountain.
        let w = 60;
        let h = 20;
        let mut heights = vec![0.1f64; w * h];

        // Mountain at columns 30..40, ramping up
        for y in 0..h {
            for x in 30..40 {
                let ramp = (x - 30) as f64 / 10.0;
                heights[y * w + x] = 0.1 + 0.7 * ramp;
            }
        }

        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        let n = w * h;
        let mut ocean_mask = vec![false; n];
        for y in 0..h {
            for x in 5..15 {
                ocean_mask[y * w + x] = true;
            }
        }
        let soil_moisture = vec![0.0f64; n];

        let mut total_precip = vec![0.0f64; n];
        for _ in 0..50 {
            let (precip, _evap) = wind.advect_moisture(&heights, &ocean_mask, &soil_moisture);
            for i in 0..precip.len() {
                total_precip[i] += precip[i];
            }
        }

        // Check precipitation on the windward slope (cols 30..35) vs leeward (cols 40..50)
        let mut windward_precip = 0.0;
        let mut leeward_precip = 0.0;
        for y in 5..15 {
            for x in 30..35 {
                windward_precip += total_precip[y * w + x];
            }
            for x in 40..50 {
                leeward_precip += total_precip[y * w + x];
            }
        }

        eprintln!("=== Orographic Rain Diagnostic ===");
        eprintln!("Windward total precip (cols 30-35): {:.4}", windward_precip);
        eprintln!("Leeward total precip (cols 40-50): {:.4}", leeward_precip);

        assert!(
            windward_precip > 0.001,
            "Windward slope should receive orographic rain, got {:.6}",
            windward_precip
        );
        assert!(
            windward_precip > leeward_precip,
            "Windward ({:.4}) should get more rain than leeward ({:.4})",
            windward_precip,
            leeward_precip
        );
    }

    /// Step 2 — Unified Hydrology: Wind evaporation from ocean.
    /// After 200 ticks with ocean on west edge and eastward wind,
    /// coastal downwind tiles should have moisture_carried > 0.1
    /// and value should decrease with distance from ocean.
    #[test]
    fn step2_ocean_evaporation_gradient() {
        let w = 60;
        let h = 20;
        let n = w * h;
        let heights = vec![0.1f64; n];
        // Eastward wind (dir=0.0 means blowing east)
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        // Ocean on west edge: columns 0..10
        let mut ocean_mask = vec![false; n];
        for y in 0..h {
            for x in 0..10 {
                ocean_mask[y * w + x] = true;
            }
        }
        let soil_moisture = vec![0.0f64; n]; // dry land, no evapotranspiration

        for _ in 0..200 {
            wind.advect_moisture(&heights, &ocean_mask, &soil_moisture);
        }

        // Coastal downwind tiles (x=12..15) should have moisture_carried > 0.1
        let mut coastal_avg = 0.0;
        for y in 5..15 {
            for x in 12..15 {
                coastal_avg += wind.moisture_carried[y * w + x];
            }
        }
        coastal_avg /= (10 * 3) as f64;

        // Far inland tiles (x=45..50) should have less moisture
        let mut inland_avg = 0.0;
        for y in 5..15 {
            for x in 45..50 {
                inland_avg += wind.moisture_carried[y * w + x];
            }
        }
        inland_avg /= (10 * 5) as f64;

        eprintln!("=== Step 2: Ocean Evaporation Gradient ===");
        eprintln!(
            "Coastal avg moisture_carried (x=12..15): {:.4}",
            coastal_avg
        );
        eprintln!("Inland avg moisture_carried (x=45..50): {:.4}", inland_avg);

        assert!(
            coastal_avg > 0.1,
            "Coastal downwind tiles should have moisture_carried > 0.1, got {:.4}",
            coastal_avg
        );
        assert!(
            coastal_avg > inland_avg,
            "Moisture should decrease with distance: coastal ({:.4}) > inland ({:.4})",
            coastal_avg,
            inland_avg
        );
    }

    /// Step 2 — Verify land evapotranspiration adds moisture from soil.
    #[test]
    fn step2_land_evapotranspiration() {
        let w = 40;
        let h = 20;
        let n = w * h;
        let heights = vec![0.1f64; n];
        let mut wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.8, None);

        let ocean_mask = vec![false; n]; // no ocean
        // Wet soil everywhere
        let soil_moisture = vec![0.5f64; n];

        // Run 100 ticks
        for _ in 0..100 {
            wind.advect_moisture(&heights, &ocean_mask, &soil_moisture);
        }

        // With evapotranspiration from moist soil, some moisture should be in the air
        let avg_carried: f64 = wind.moisture_carried.iter().sum::<f64>() / n as f64;

        eprintln!("=== Step 2: Land Evapotranspiration ===");
        eprintln!("Avg moisture_carried with wet soil: {:.6}", avg_carried);

        assert!(
            avg_carried > 0.001,
            "Wet soil should evapotranspire some moisture into air, got {:.6}",
            avg_carried
        );
    }
}
