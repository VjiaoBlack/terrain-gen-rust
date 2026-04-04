//! Step 6 — Unified Hydrology Integration Test
//!
//! Validates the ENTIRE hydrology chain end-to-end on a 64x64 map:
//!   ocean boundary → evaporation → wind transport → orographic precipitation
//!   → tile moisture → proportional vegetation growth → rain shadow
//!
//! Reference: docs/design/cross_cutting/unified_hydrology.md

use terrain_gen_rust::pipe_water::PipeWater;
use terrain_gen_rust::simulation::moisture::MoistureMap;
use terrain_gen_rust::simulation::vegetation::VegetationMap;
use terrain_gen_rust::simulation::wind::WindField;
use terrain_gen_rust::tilemap::{Terrain, TileMap};

/// Build the 64x64 test scenario:
/// - Ocean on west edge (x=0..5): height 0.35 (below water_level 0.42)
/// - Flat land (x=5..20): height 0.4
/// - Mountain ridge centered at x=22 (Gaussian profile, peak ~0.6)
/// - Flat land east (x=28..60): height 0.4
/// - Wind blowing east (prevailing direction = 0.0)
///
/// The ridge uses a smooth Gaussian shape so the Stam fluid solver
/// produces proper orographic lift rather than deflecting around a wall.
/// Heights are kept relatively close to avoid the Stam solver treating
/// the ocean-to-land transition as a terrain barrier.
fn build_scenario() -> (
    Vec<f64>, // heights
    TileMap,
    PipeWater,
    MoistureMap,
    VegetationMap,
    WindField,
) {
    let w = 64;
    let h = 64;
    let n = w * h;

    // Heights: ocean floor west, flat land, smooth Gaussian ridge, flat land east.
    // Heights are kept close together so the Stam solver doesn't treat the
    // ocean-to-land transition as a terrain wall.  The ridge is a moderate
    // Gaussian bump (peak ~0.6) that creates orographic lift.
    let mut heights = vec![0.4; n];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x < 5 {
                heights[i] = 0.35; // ocean floor (below water_level 0.42)
            } else {
                // Gaussian mountain ridge centered at x=22
                let dist = (x as f64 - 22.0).abs();
                let ridge = 0.2 * (-dist * dist / 18.0).exp();
                heights[i] = 0.4 + ridge;
            }
        }
    }

    // TileMap: ocean on x=0..5, grass elsewhere
    let mut map = TileMap::new(w, h, Terrain::Grass);
    for y in 0..h {
        for x in 0..5 {
            map.set(x, y, Terrain::Water);
        }
    }

    // PipeWater with ocean boundary
    let mut pw = PipeWater::new(w, h);
    for y in 0..h {
        for x in 0..5 {
            let ocean_depth = 0.07; // water_level(0.42) - height(0.35)
            pw.set_ocean_boundary(x, y, ocean_depth);
            pw.add_water(x, y, ocean_depth);
        }
    }

    let mm = MoistureMap::new(w, h);
    let vm = VegetationMap::new(w, h);

    // Wind blowing east (prevailing_dir = 0.0 radians)
    let wind = WindField::compute_from_terrain(&heights, w, h, 0.0, 0.6, None);

    (heights, map, pw, mm, vm, wind)
}

#[test]
fn step6_unified_hydrology_end_to_end() {
    let w = 64;
    let h = 64;
    let (heights, map, mut pw, mut mm, mut vm, mut wind) = build_scenario();

    // Record initial ocean depths for assertion 1
    let mut initial_ocean_depths = Vec::new();
    for y in 0..h {
        for x in 0..5 {
            let i = y * w + x;
            initial_ocean_depths.push((x, y, pw.depth[i]));
        }
    }

    // Run for 500 ticks of hydrology, then continue to 1500 total to let
    // vegetation respond to accumulated moisture gradients.
    // Wind advection every 3 ticks matches the game loop cadence.
    for tick in 0..1500 {
        if tick % 3 == 0 {
            wind.advect_moisture(&heights, &pw.ocean_mask, &mm.moisture);
        }
        // Precipitation + vegetation update every tick
        mm.update(&mut pw, &mut vm, &map, &mut wind, &heights);
        // Pipe water step
        pw.step(&heights, 0.02);
    }

    // =========================================================
    // Assertion 1: Ocean depths constant (within 1% of initial)
    //   Validates Step 1 — ocean boundary condition
    // =========================================================
    for &(x, y, initial_depth) in &initial_ocean_depths {
        let i = y * w + x;
        let current_depth = pw.depth[i];
        let diff = (current_depth - initial_depth).abs();
        let tolerance = initial_depth * 0.01;
        assert!(
            diff <= tolerance,
            "Ocean tile ({},{}) depth changed: initial={:.4}, current={:.4}, diff={:.4} > 1%",
            x,
            y,
            initial_depth,
            current_depth,
            diff,
        );
    }

    // Measure regional averages for west-of-ridge (windward) and east-of-ridge (leeward)
    let mut west_moisture_sum = 0.0;
    let mut west_count = 0;
    let mut east_moisture_sum = 0.0;
    let mut east_count = 0;
    let mut west_veg_sum = 0.0;
    let mut east_veg_sum = 0.0;
    let mut west_carried_sum = 0.0;
    let mut east_carried_sum = 0.0;

    for y in 0..h {
        for x in 5..20 {
            west_moisture_sum += mm.get(x, y);
            west_veg_sum += vm.get(x, y);
            west_carried_sum += wind.get_moisture_carried(x, y);
            west_count += 1;
        }
        for x in 28..60 {
            east_moisture_sum += mm.get(x, y);
            east_veg_sum += vm.get(x, y);
            east_carried_sum += wind.get_moisture_carried(x, y);
            east_count += 1;
        }
    }

    let west_avg_moisture = west_moisture_sum / west_count as f64;
    let east_avg_moisture = east_moisture_sum / east_count as f64;
    let west_avg_veg = west_veg_sum / west_count as f64;
    let east_avg_veg = east_veg_sum / east_count as f64;
    let west_avg_carried = west_carried_sum / west_count as f64;
    let east_avg_carried = east_carried_sum / east_count as f64;

    // Print diagnostics
    eprintln!("\n=== Unified Hydrology Integration Test (1500 ticks) ===");
    eprintln!(
        "Soil moisture: west={:.4}, east={:.4} (ratio {:.2}x)",
        west_avg_moisture,
        east_avg_moisture,
        west_avg_moisture / east_avg_moisture.max(0.0001)
    );
    eprintln!(
        "Vegetation:    west={:.6}, east={:.6}",
        west_avg_veg, east_avg_veg
    );
    eprintln!(
        "Atm. carried:  west={:.6}, east={:.6}",
        west_avg_carried, east_avg_carried
    );

    // =========================================================
    // Assertion 2: Windward side gets meaningful moisture
    //   Validates: evaporation → wind transport → precipitation chain
    // =========================================================
    assert!(
        west_avg_moisture > 0.2,
        "Windward avg moisture should be > 0.2, got {:.4}",
        west_avg_moisture
    );

    // =========================================================
    // Assertion 3: Rain shadow — east drier than west
    //   Validates: orographic precipitation depletes moisture before ridge
    // =========================================================
    assert!(
        east_avg_moisture < west_avg_moisture,
        "Rain shadow: east should be drier than west: east={:.4}, west={:.4}",
        east_avg_moisture,
        west_avg_moisture
    );

    // =========================================================
    // Assertion 4: Vegetation follows moisture — west > east
    //   Validates: proportional vegetation growth responds to moisture
    // =========================================================
    assert!(
        west_avg_veg > east_avg_veg,
        "Windward vegetation should exceed leeward: west={:.6}, east={:.6}",
        west_avg_veg,
        east_avg_veg
    );

    // =========================================================
    // Assertion 5: Atmospheric moisture lower east of ridge
    //   Validates: moisture consumed by precipitation on windward slopes
    // =========================================================
    assert!(
        east_avg_carried < west_avg_carried,
        "Atmospheric moisture should be lower east of ridge: east={:.6}, west={:.6}",
        east_avg_carried,
        west_avg_carried
    );
}
