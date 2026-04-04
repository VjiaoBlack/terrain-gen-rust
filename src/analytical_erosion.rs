//! Analytical Stream Power Law (SPL) erosion.
//!
//! Replaces particle-based droplet erosion with a closed-form solution:
//!   erosion_rate = K * A^m * S^n
//! where A = upstream drainage area, S = local slope.
//!
//! Key advantage: SPL is pure incision — it does not deposit silt into
//! ocean basins or enclosed depressions.

use crate::terrain_pipeline::{compute_flow_accumulation, compute_flow_direction};

/// SPL tuning parameters.
#[derive(Clone, Debug)]
pub struct SplParams {
    /// Erosion coefficient (rock erodibility). Typical: 0.001
    pub k: f64,
    /// Area exponent. Typical: 0.5
    pub m: f64,
    /// Slope exponent. Typical: 1.0
    pub n: f64,
    /// Time / intensity slider. Higher = more erosion.
    pub time: f64,
    /// Tiles at or below this height are ocean and will not be eroded.
    pub water_level: f64,
}

impl Default for SplParams {
    fn default() -> Self {
        Self {
            k: 0.001,
            m: 0.5,
            n: 1.0,
            time: 1.0,
            water_level: 0.35,
        }
    }
}

/// Compute per-tile upstream drainage area using D8 flow routing.
///
/// Delegates to `compute_flow_direction` + `compute_flow_accumulation`
/// which already exist in `terrain_pipeline`.
pub fn compute_drainage_area(heights: &[f64], w: usize, h: usize) -> Vec<f64> {
    let flow = compute_flow_direction(heights, w, h);
    compute_flow_accumulation(heights, &flow, w, h)
}

/// Apply Stream Power Law erosion in-place.
///
/// For each land tile (above water_level):
///   slope = max drop to any D8 neighbor
///   erosion = K * A^m * S^n * time
///   height -= erosion
///
/// Ocean tiles (height <= water_level) are left untouched.
pub fn apply_spl_erosion(
    heights: &mut [f64],
    w: usize,
    h: usize,
    drainage: &[f64],
    params: &SplParams,
) {
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
    let dist = [
        1.0,
        1.0,
        1.0,
        1.0,
        std::f64::consts::FRAC_1_SQRT_2.recip(),
        std::f64::consts::FRAC_1_SQRT_2.recip(),
        std::f64::consts::FRAC_1_SQRT_2.recip(),
        std::f64::consts::FRAC_1_SQRT_2.recip(),
    ];

    // Compute erosion amounts into a separate buffer so reads are consistent.
    let n = w * h;
    let mut erosion = vec![0.0f64; n];

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;

            // Skip ocean tiles
            if heights[i] <= params.water_level {
                continue;
            }

            // Find steepest downhill slope (D8)
            let mut max_slope = 0.0f64;
            for (di, &(dx, dy)) in dirs.iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                let s = (heights[i] - heights[ni]) / dist[di];
                if s > max_slope {
                    max_slope = s;
                }
            }

            // SPL: erosion_rate = K * A^m * S^n
            if max_slope > 0.0 {
                let area = drainage[i];
                let rate = params.k * area.powf(params.m) * max_slope.powf(params.n);
                erosion[i] = rate * params.time;
            }
        }
    }

    // Apply erosion (don't erode below water level)
    for i in 0..n {
        if erosion[i] > 0.0 {
            heights[i] -= erosion[i];
            if heights[i] < params.water_level {
                heights[i] = params.water_level;
            }
        }
    }
}

/// Convenience: compute drainage + apply SPL in one call.
/// Uses multi-pass iteration: recomputes drainage between passes so the
/// terrain relaxes gradually instead of creating deep single-pixel gullies.
pub fn run_spl_erosion(heights: &mut [f64], w: usize, h: usize, params: &SplParams) {
    let passes = 4;
    let per_pass = SplParams {
        time: params.time / passes as f64,
        ..params.clone()
    };
    for _ in 0..passes {
        let drainage = compute_drainage_area(heights, w, h);
        apply_spl_erosion(heights, w, h, &drainage, &per_pass);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a flat heightmap at the given elevation.
    fn flat_map(w: usize, h: usize, elevation: f64) -> Vec<f64> {
        vec![elevation; w * h]
    }

    // ── Test 1: Flat terrain produces zero erosion ──────────────────────────

    #[test]
    fn flat_terrain_no_erosion() {
        let w = 16;
        let h = 16;
        let mut heights = flat_map(w, h, 0.6);
        let original = heights.clone();
        let params = SplParams::default();
        run_spl_erosion(&mut heights, w, h, &params);
        assert_eq!(heights, original, "flat terrain should not erode (slope=0)");
    }

    // ── Test 2: Valley floor (high drainage) erodes more than ridgetop ─────

    #[test]
    fn high_drainage_erodes_more_than_low_drainage() {
        // Create a simple ramp (slope toward y=0). After computing drainage,
        // cells near y=0 have high drainage (all upstream cells drain to them)
        // while cells near y=max have low drainage (only themselves).
        // Both have the same slope, so the difference in erosion is purely
        // from drainage area (A^m term).
        let w = 16;
        let h = 32;
        let mut heights = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                // Uniform slope: height increases linearly with y
                heights[y * w + x] = 0.4 + (y as f64 / h as f64) * 0.4;
            }
        }

        let drainage = compute_drainage_area(&heights, w, h);
        let params = SplParams {
            k: 0.01,
            time: 1.0,
            water_level: 0.0,
            ..SplParams::default()
        };

        let original = heights.clone();
        apply_spl_erosion(&mut heights, w, h, &drainage, &params);

        // Cell near outlet (y=1, center) — high drainage from upstream rows.
        // Cell near headwater (y=h-2, center) — low drainage (only 1 row above).
        let outlet_idx = 1 * w + w / 2;
        let headwater_idx = (h - 2) * w + w / 2;

        let outlet_erosion = original[outlet_idx] - heights[outlet_idx];
        let headwater_erosion = original[headwater_idx] - heights[headwater_idx];

        assert!(
            drainage[outlet_idx] > drainage[headwater_idx],
            "outlet drainage ({:.1}) should exceed headwater ({:.1})",
            drainage[outlet_idx],
            drainage[headwater_idx],
        );
        assert!(
            outlet_erosion > headwater_erosion,
            "outlet (drainage={:.1}, erosion={:.6}) should erode more than headwater (drainage={:.1}, erosion={:.6})",
            drainage[outlet_idx],
            outlet_erosion,
            drainage[headwater_idx],
            headwater_erosion,
        );
    }

    // ── Test 3: Ocean tiles are not eroded ──────────────────────────────────

    #[test]
    fn ocean_tiles_not_eroded() {
        let w = 16;
        let h = 16;
        let water_level = 0.35;

        // Left half: ocean (below water_level). Right half: land slope.
        let mut heights = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                if x < w / 2 {
                    heights[y * w + x] = 0.2; // ocean
                } else {
                    heights[y * w + x] = 0.4 + (x as f64 / w as f64) * 0.3; // land slope
                }
            }
        }

        let original_ocean: Vec<f64> = (0..h)
            .flat_map(|y| (0..w / 2).map(move |x| (y, x)))
            .map(|(y, x)| heights[y * w + x])
            .collect();

        let params = SplParams {
            water_level,
            k: 0.01,
            time: 5.0,
            ..SplParams::default()
        };
        run_spl_erosion(&mut heights, w, h, &params);

        let after_ocean: Vec<f64> = (0..h)
            .flat_map(|y| (0..w / 2).map(move |x| (y, x)))
            .map(|(y, x)| heights[y * w + x])
            .collect();

        assert_eq!(
            original_ocean, after_ocean,
            "ocean tiles should remain unchanged"
        );
    }

    // ── Test 4: Erosion is proportional to time parameter ───────────────────

    #[test]
    fn erosion_proportional_to_time() {
        let w = 16;
        let h = 16;
        // Simple slope: elevation increases with x.
        let make_slope = || -> Vec<f64> {
            let mut heights = vec![0.0; w * h];
            for y in 0..h {
                for x in 0..w {
                    heights[y * w + x] = 0.4 + (x as f64 / w as f64) * 0.4;
                }
            }
            heights
        };

        let mut h1 = make_slope();
        let mut h2 = make_slope();
        let original = make_slope();

        let drainage = compute_drainage_area(&original, w, h);

        let p1 = SplParams {
            time: 1.0,
            water_level: 0.0,
            k: 0.01,
            ..SplParams::default()
        };
        let p2 = SplParams {
            time: 2.0,
            water_level: 0.0,
            k: 0.01,
            ..SplParams::default()
        };

        apply_spl_erosion(&mut h1, w, h, &drainage, &p1);
        apply_spl_erosion(&mut h2, w, h, &drainage, &p2);

        let total_erosion_1: f64 = original.iter().zip(h1.iter()).map(|(o, a)| o - a).sum();
        let total_erosion_2: f64 = original.iter().zip(h2.iter()).map(|(o, a)| o - a).sum();

        // With small K the erosion won't hit the water_level floor,
        // so total_erosion_2 should be ~2x total_erosion_1.
        let ratio = total_erosion_2 / total_erosion_1;
        assert!(
            (ratio - 2.0).abs() < 0.05,
            "double time should double total erosion, but ratio was {ratio:.4}"
        );
    }

    // ── Test 5: Steeper slopes erode faster ─────────────────────────────────

    #[test]
    fn steeper_slopes_erode_faster() {
        let w = 16;
        let h = 16;

        // Two regions: left half has gentle slope, right half has steep slope.
        let mut heights = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let base = 0.5;
                if x < w / 2 {
                    // gentle: 0.5 to 0.6
                    heights[y * w + x] = base + (x as f64 / w as f64) * 0.2;
                } else {
                    // steep: 0.5 to 0.9
                    heights[y * w + x] = base + (x as f64 / w as f64) * 0.8;
                }
            }
        }

        let drainage = compute_drainage_area(&heights, w, h);
        let params = SplParams {
            k: 0.01,
            time: 1.0,
            water_level: 0.0,
            ..SplParams::default()
        };

        let original = heights.clone();
        apply_spl_erosion(&mut heights, w, h, &drainage, &params);

        // Sum erosion in gentle vs steep half.
        let mut erosion_gentle = 0.0;
        let mut erosion_steep = 0.0;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let e = original[i] - heights[i];
                if x < w / 2 {
                    erosion_gentle += e;
                } else {
                    erosion_steep += e;
                }
            }
        }

        assert!(
            erosion_steep > erosion_gentle,
            "steep region ({erosion_steep:.6}) should erode more than gentle ({erosion_gentle:.6})"
        );
    }

    // ── Test 6: Drainage area computation sanity check ──────────────────────

    #[test]
    fn drainage_area_increases_downstream() {
        // Simple ramp: water flows from high y to low y.
        let w = 8;
        let h = 8;
        let mut heights = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                heights[y * w + x] = y as f64 / h as f64; // increases with y
            }
        }

        let drainage = compute_drainage_area(&heights, w, h);

        // Bottom row (y=0) is the outlet — should have highest drainage.
        let bottom_avg: f64 = (0..w).map(|x| drainage[x]).sum::<f64>() / w as f64;
        let top_avg: f64 = (0..w).map(|x| drainage[(h - 1) * w + x]).sum::<f64>() / w as f64;

        assert!(
            bottom_avg > top_avg,
            "bottom row (avg drainage {bottom_avg:.1}) should have more drainage than top ({top_avg:.1})"
        );
    }

}
