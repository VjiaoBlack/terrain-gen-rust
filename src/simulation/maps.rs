use serde::{Deserialize, Serialize};

use super::scent::ScentMap;

/// Influence map for territory visualization. Each villager and building emits
/// influence that diffuses outward, creating an organic territory boundary.
#[derive(Serialize, Deserialize)]
pub struct InfluenceMap {
    pub width: usize,
    pub height: usize,
    influence: Vec<f64>,
}

impl InfluenceMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            influence: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.influence[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Update: decay all cells slightly, then add influence from sources, then diffuse.
    /// sources: (x, y, strength) — villagers emit 1.0, buildings emit 0.5
    /// `viewport` is an optional `(x_start, y_start, x_end, y_end)` bounds; when Some, only
    /// tiles within the viewport plus a 32-tile margin are processed.
    pub fn update(
        &mut self,
        sources: &[(f64, f64, f64)],
        viewport: Option<(usize, usize, usize, usize)>,
    ) {
        let (y_lo, y_hi, x_lo, x_hi) = match viewport {
            Some((xs, ys, xe, ye)) => (
                ys.saturating_sub(32),
                ye.saturating_add(32).min(self.height),
                xs.saturating_sub(32),
                xe.saturating_add(32).min(self.width),
            ),
            None => (0, self.height, 0, self.width),
        };

        // Decay existing influence (within bounds)
        for y in y_lo..y_hi {
            for x in x_lo..x_hi {
                self.influence[y * self.width + x] *= 0.98;
            }
        }

        // Add from sources (only those within bounds)
        for &(sx, sy, strength) in sources {
            let ix = sx.round() as usize;
            let iy = sy.round() as usize;
            if ix >= x_lo && ix < x_hi && iy >= y_lo && iy < y_hi {
                self.influence[iy * self.width + ix] += strength;
            }
        }

        // Simple diffusion: average with neighbors (within bounds, skipping edges)
        let mut temp = self.influence.clone();
        let diff_y_lo = y_lo.max(1);
        let diff_y_hi = y_hi.min(self.height.saturating_sub(1));
        let diff_x_lo = x_lo.max(1);
        let diff_x_hi = x_hi.min(self.width.saturating_sub(1));
        for y in diff_y_lo..diff_y_hi {
            for x in diff_x_lo..diff_x_hi {
                let idx = y * self.width + x;
                let avg = (self.influence[idx] * 4.0
                    + self.influence[idx - 1]
                    + self.influence[idx + 1]
                    + self.influence[(y - 1) * self.width + x]
                    + self.influence[(y + 1) * self.width + x])
                    / 8.0;
                temp[idx] = avg;
            }
        }
        self.influence = temp;
    }
}

/// Per-tile threat/defense data for the Threats overlay.
///
/// Stores wolf territory zones, garrison coverage radii, approach corridor
/// pressure, and computed exposure gaps. Updated periodically (every 100 ticks)
/// and whenever garrisons are built/destroyed.
pub struct ThreatMap {
    pub width: usize,
    pub height: usize,
    /// 0.0 = safe, 1.0 = core wolf territory (forest in qualifying cluster).
    /// 0.5 = buffer zone (within 3 tiles of qualifying cluster edge).
    pub wolf_territory: Vec<f32>,
    /// 0.0 = no corridor, 1.0 = primary approach through undefended chokepoint.
    pub corridor_pressure: Vec<f32>,
    /// 0.0 = uncovered, values grow with garrison proximity. Multiple garrisons stack.
    pub garrison_coverage: Vec<f32>,
    /// Computed: wolf_territory + corridor_pressure - garrison_coverage, clamped >= 0.
    pub exposure: Vec<f32>,
}

impl ThreatMap {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            wolf_territory: vec![0.0; n],
            corridor_pressure: vec![0.0; n],
            garrison_coverage: vec![0.0; n],
            exposure: vec![0.0; n],
        }
    }

    /// Get wolf territory value at (x, y).
    pub fn wolf_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.wolf_territory[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get garrison coverage at (x, y).
    pub fn garrison_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.garrison_coverage[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get corridor pressure at (x, y).
    pub fn corridor_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.corridor_pressure[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get exposure gap at (x, y).
    pub fn exposure_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.exposure[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Recompute wolf territory from the terrain map and danger scent.
    /// Forest tiles with significant danger scent nearby are core territory (1.0).
    /// Forest tiles within 15-60 tiles of the settlement center with cluster size > 20
    /// get territory marking. We approximate this using danger scent as a proxy for
    /// wolf presence (wolves emit danger scent where they live).
    pub fn update_wolf_territory(
        &mut self,
        map: &crate::tilemap::TileMap,
        danger_scent: &ScentMap,
        settlement_center: (i32, i32),
    ) {
        use crate::tilemap::Terrain;
        self.wolf_territory.fill(0.0);
        let (scx, scy) = settlement_center;
        let w = self.width;
        let h = self.height;

        // Pass 1: mark forest tiles that have danger scent as core wolf territory
        for y in 0..h {
            for x in 0..w {
                let terrain = map.get(x, y).copied().unwrap_or(Terrain::Water);
                if terrain != Terrain::Forest {
                    continue;
                }
                let dist_to_settlement =
                    (((x as i32 - scx).pow(2) + (y as i32 - scy).pow(2)) as f64).sqrt();
                // Only mark forests within relevant range (10-80 tiles from settlement)
                if dist_to_settlement < 10.0 || dist_to_settlement > 80.0 {
                    continue;
                }
                let scent = danger_scent.get(x, y);
                if scent > 0.05 {
                    self.wolf_territory[y * w + x] = 1.0;
                } else if scent > 0.01 {
                    self.wolf_territory[y * w + x] = 0.5;
                }
            }
        }

        // Pass 2: buffer zone — mark non-forest tiles within 3 tiles of wolf territory
        // Use a simple expansion pass
        let snapshot: Vec<f32> = self.wolf_territory.clone();
        for y in 0..h {
            for x in 0..w {
                if snapshot[y * w + x] > 0.0 {
                    continue; // already marked
                }
                // Check 3-tile neighborhood for wolf territory
                let mut nearest_dist_sq = u32::MAX;
                for dy in -3i32..=3 {
                    for dx in -3i32..=3 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            if snapshot[ny as usize * w + nx as usize] >= 1.0 {
                                let d = (dx * dx + dy * dy) as u32;
                                if d < nearest_dist_sq {
                                    nearest_dist_sq = d;
                                }
                            }
                        }
                    }
                }
                if nearest_dist_sq <= 9 {
                    // Within 3 tiles
                    self.wolf_territory[y * w + x] =
                        0.3 * (1.0 - (nearest_dist_sq as f32).sqrt() / 3.0);
                }
            }
        }
    }

    /// Recompute garrison coverage from garrison positions.
    /// Each garrison radiates coverage that decays with distance (radius 12 base).
    /// Garrisons near chokepoints get a bonus radius.
    pub fn update_garrison_coverage(
        &mut self,
        garrisons: &[(usize, usize)],
        chokepoint_scores: &[f64],
    ) {
        self.garrison_coverage.fill(0.0);
        let w = self.width;
        let h = self.height;
        let base_radius: i32 = 12;

        for &(gx, gy) in garrisons {
            // Check if garrison is near a chokepoint (score > 0.2)
            let choke_score = if gx < w && gy < h {
                chokepoint_scores.get(gy * w + gx).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            let bonus = if choke_score > 0.2 { 5 } else { 0 };
            let radius = base_radius + bonus;
            let defense_bonus: f32 = 1.0 + choke_score as f32 * 0.3;

            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let tx = gx as i32 + dx;
                    let ty = gy as i32 + dy;
                    if tx < 0 || ty < 0 || tx as usize >= w || ty as usize >= h {
                        continue;
                    }
                    let dist = ((dx * dx + dy * dy) as f64).sqrt();
                    if dist > radius as f64 {
                        continue;
                    }
                    let coverage = defense_bonus / (1.0 + dist as f32 * 0.15);
                    self.garrison_coverage[ty as usize * w + tx as usize] += coverage;
                }
            }
        }
    }

    /// Recompute corridor pressure from chokepoint data.
    /// High-scoring chokepoint tiles that lack garrison coverage get pressure.
    pub fn update_corridor_pressure(&mut self, chokepoint_scores: &[f64]) {
        let n = self.width * self.height;
        self.corridor_pressure.fill(0.0);
        if chokepoint_scores.len() != n {
            return;
        }
        for i in 0..n {
            let score = chokepoint_scores[i] as f32;
            if score > 0.1 {
                self.corridor_pressure[i] = score;
            }
        }
    }

    /// Recompute exposure = threat - defense, clamped to [0, 1].
    pub fn recompute_exposure(&mut self) {
        let n = self.width * self.height;
        for i in 0..n {
            let threat = self.wolf_territory[i] + self.corridor_pressure[i];
            let defense = self.garrison_coverage[i];
            self.exposure[i] = (threat - defense).clamp(0.0, 1.0);
        }
    }

    /// Full update: recompute all layers and exposure.
    pub fn update(
        &mut self,
        map: &crate::tilemap::TileMap,
        danger_scent: &ScentMap,
        settlement_center: (i32, i32),
        garrisons: &[(usize, usize)],
        chokepoint_scores: &[f64],
    ) {
        self.update_wolf_territory(map, danger_scent, settlement_center);
        self.update_garrison_coverage(garrisons, chokepoint_scores);
        self.update_corridor_pressure(chokepoint_scores);
        self.recompute_exposure();
    }
}

impl Default for ThreatMap {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// Tracks which tiles have been explored (revealed) by creatures.
/// Unexplored tiles are rendered as dark fog.
pub struct ExplorationMap {
    pub revealed: Vec<bool>,
    pub width: usize,
    pub height: usize,
}

impl ExplorationMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            revealed: vec![false; width * height],
            width,
            height,
        }
    }

    /// Mark all tiles within `radius` of (cx, cy) as revealed.
    /// Uses simple Euclidean distance check (no raycasting).
    pub fn reveal(&mut self, cx: usize, cy: usize, radius: usize) {
        let r = radius as i32;
        let r_sq = (radius * radius) as i32;
        let min_x = (cx as i32 - r).max(0) as usize;
        let max_x = ((cx as i32 + r) as usize).min(self.width.saturating_sub(1));
        let min_y = (cy as i32 - r).max(0) as usize;
        let max_y = ((cy as i32 + r) as usize).min(self.height.saturating_sub(1));
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as i32 - cx as i32;
                let dy = y as i32 - cy as i32;
                if dx * dx + dy * dy <= r_sq {
                    self.revealed[y * self.width + x] = true;
                }
            }
        }
    }

    /// Returns true if the tile at (x, y) has been revealed.
    pub fn is_revealed(&self, x: usize, y: usize) -> bool {
        if x < self.width && y < self.height {
            self.revealed[y * self.width + x]
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilemap::{Terrain, TileMap};

    #[test]
    fn influence_map_diffuses() {
        let mut im = InfluenceMap::new(10, 10);
        // Add a source at center
        im.update(&[(5.0, 5.0, 5.0)], None);

        // Center should have influence
        assert!(
            im.get(5, 5) > 0.0,
            "center should have influence after source: got {}",
            im.get(5, 5)
        );

        // Run more ticks to let it diffuse
        for _ in 0..20 {
            im.update(&[(5.0, 5.0, 1.0)], None);
        }

        // Neighbors should have picked up some influence via diffusion
        assert!(
            im.get(4, 5) > 0.0,
            "left neighbor should have influence via diffusion: got {}",
            im.get(4, 5)
        );
        assert!(
            im.get(6, 5) > 0.0,
            "right neighbor should have influence via diffusion: got {}",
            im.get(6, 5)
        );
        assert!(
            im.get(5, 4) > 0.0,
            "top neighbor should have influence via diffusion: got {}",
            im.get(5, 4)
        );
        assert!(
            im.get(5, 6) > 0.0,
            "bottom neighbor should have influence via diffusion: got {}",
            im.get(5, 6)
        );

        // Center should be stronger than edges
        assert!(
            im.get(5, 5) > im.get(1, 1),
            "center should be stronger than corner"
        );
    }

    #[test]
    fn influence_map_decays() {
        let mut im = InfluenceMap::new(10, 10);
        // Add strong source once
        im.update(&[(5.0, 5.0, 10.0)], None);
        let initial = im.get(5, 5);
        assert!(initial > 0.0);

        // Update many times with no sources — should decay
        for _ in 0..200 {
            im.update(&[], None);
        }

        let after = im.get(5, 5);
        assert!(
            after < initial * 0.1,
            "influence should decay significantly without sources: initial={} after={}",
            initial,
            after
        );
    }

    #[test]
    fn viewport_influence_matches_full_in_overlap() {
        let size = 20;
        let mut im_full = InfluenceMap::new(size, size);
        let mut im_vp = InfluenceMap::new(size, size);

        let sources = vec![(10.0, 10.0, 5.0)];

        // With a 20x20 map and viewport (5,5,15,15), margin of 32 covers the whole map.
        // So results should match exactly.
        let viewport = Some((5, 5, 15, 15));

        for _ in 0..10 {
            im_full.update(&sources, None);
            im_vp.update(&sources, viewport);
        }

        // Check overlap region
        for y in 5..15 {
            for x in 5..15 {
                let diff = (im_full.get(x, y) - im_vp.get(x, y)).abs();
                assert!(
                    diff < 1e-10,
                    "influence mismatch at ({}, {}): full={} vp={}",
                    x,
                    y,
                    im_full.get(x, y),
                    im_vp.get(x, y)
                );
            }
        }
    }

    #[test]
    fn viewport_influence_restricts_to_bounds() {
        // On a large map, viewport should not update tiles far outside the margin.
        let size = 128;
        let mut im = InfluenceMap::new(size, size);

        // Seed some influence everywhere
        for v in im.influence.iter_mut() {
            *v = 1.0;
        }

        let initial_val = im.get(0, 0);

        // Viewport at far end: (100, 100, 120, 120), margin -> (68, 68, 128, 128)
        // So (0, 0) is outside.
        let viewport = Some((100, 100, 120, 120));

        im.update(&[], viewport);

        // Tile at (0, 0) should not have decayed (it's outside the bounds)
        assert_eq!(
            im.get(0, 0),
            initial_val,
            "influence outside viewport+margin should not decay"
        );
    }

    // ---- ThreatMap tests ----

    #[test]
    fn threat_map_new_dimensions() {
        let tm = ThreatMap::new(10, 20);
        assert_eq!(tm.width, 10);
        assert_eq!(tm.height, 20);
        assert_eq!(tm.wolf_territory.len(), 200);
        assert_eq!(tm.garrison_coverage.len(), 200);
        assert_eq!(tm.corridor_pressure.len(), 200);
        assert_eq!(tm.exposure.len(), 200);
    }

    #[test]
    fn threat_map_default_is_empty() {
        let tm = ThreatMap::default();
        assert_eq!(tm.width, 0);
        assert_eq!(tm.height, 0);
    }

    #[test]
    fn threat_map_wolf_territory_marks_forest_with_scent() {
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        // Place a cluster of forest tiles 20 tiles from center (15,15)
        for y in 3..8 {
            for x in 3..8 {
                map.set(x, y, Terrain::Forest);
            }
        }
        let mut scent = ScentMap::new(30, 30, 0.998, 0.0);
        // Emit danger scent on the forest tiles (wolf presence)
        for y in 3..8 {
            for x in 3..8 {
                scent.emit(x, y, 1.0);
            }
        }

        let mut tm = ThreatMap::new(30, 30);
        tm.update_wolf_territory(&map, &scent, (15, 15));

        // Forest tiles with scent should have wolf territory > 0
        assert!(
            tm.wolf_at(5, 5) > 0.0,
            "forest tile with scent should be wolf territory"
        );
        // Grass tile far from forest should be 0
        assert!(
            tm.wolf_at(20, 20) == 0.0,
            "grass tile far from forest should have no wolf territory"
        );
    }

    #[test]
    fn threat_map_wolf_buffer_zone() {
        let mut map = TileMap::new(40, 40, Terrain::Grass);
        for y in 10..15 {
            for x in 10..15 {
                map.set(x, y, Terrain::Forest);
            }
        }
        let mut scent = ScentMap::new(40, 40, 0.998, 0.0);
        for y in 10..15 {
            for x in 10..15 {
                scent.emit(x, y, 1.0);
            }
        }

        let mut tm = ThreatMap::new(40, 40);
        tm.update_wolf_territory(&map, &scent, (20, 20));

        // Buffer zone: within 3 tiles of forest edge
        let buffer_val = tm.wolf_at(8, 12); // 2 tiles from forest edge (x=10)
        assert!(
            buffer_val > 0.0,
            "tile near wolf territory should have buffer value, got {}",
            buffer_val
        );
        // Far away: no buffer
        assert_eq!(
            tm.wolf_at(1, 1),
            0.0,
            "tile far from wolf territory should have no buffer"
        );
    }

    #[test]
    fn threat_map_garrison_coverage_decays_with_distance() {
        let mut tm = ThreatMap::new(30, 30);
        let garrisons = vec![(15, 15)];
        let scores = vec![0.0; 30 * 30]; // no chokepoints

        tm.update_garrison_coverage(&garrisons, &scores);

        let close = tm.garrison_at(15, 15);
        let mid = tm.garrison_at(15, 20); // 5 tiles away
        let far = tm.garrison_at(15, 26); // 11 tiles away

        assert!(close > mid, "coverage should decrease with distance");
        assert!(mid > far, "coverage should decrease further with distance");
        assert!(far > 0.0, "coverage should still exist within radius");

        // Beyond radius
        let beyond = tm.garrison_at(15, 28); // 13 tiles away, beyond base 12
        assert_eq!(
            beyond, 0.0,
            "beyond garrison radius should have no coverage"
        );
    }

    #[test]
    fn threat_map_garrison_chokepoint_bonus() {
        let mut tm = ThreatMap::new(30, 30);
        let garrisons = vec![(15, 15)];
        let mut scores = vec![0.0; 30 * 30];
        // Set high chokepoint score at garrison position
        scores[15 * 30 + 15] = 0.5;

        tm.update_garrison_coverage(&garrisons, &scores);
        let with_bonus = tm.garrison_at(15, 15);

        // Recompute without bonus
        let mut tm2 = ThreatMap::new(30, 30);
        let scores_none = vec![0.0; 30 * 30];
        tm2.update_garrison_coverage(&garrisons, &scores_none);
        let without_bonus = tm2.garrison_at(15, 15);

        assert!(
            with_bonus > without_bonus,
            "chokepoint garrison should have higher coverage ({} vs {})",
            with_bonus,
            without_bonus
        );
    }

    #[test]
    fn threat_map_multiple_garrisons_stack() {
        let mut tm = ThreatMap::new(30, 30);
        let garrisons = vec![(12, 15), (18, 15)];
        let scores = vec![0.0; 30 * 30];

        tm.update_garrison_coverage(&garrisons, &scores);
        let overlap = tm.garrison_at(15, 15); // midpoint between two garrisons

        let mut tm_single = ThreatMap::new(30, 30);
        tm_single.update_garrison_coverage(&[(12, 15)].to_vec(), &scores);
        let single = tm_single.garrison_at(15, 15);

        assert!(
            overlap > single,
            "overlapping garrison coverage should exceed single ({} vs {})",
            overlap,
            single
        );
    }

    #[test]
    fn threat_map_corridor_pressure_from_chokepoints() {
        let mut tm = ThreatMap::new(10, 10);
        let mut scores = vec![0.0; 100];
        scores[55] = 0.8; // tile (5,5) is a chokepoint

        tm.update_corridor_pressure(&scores);

        assert!(
            tm.corridor_at(5, 5) > 0.0,
            "chokepoint tile should have corridor pressure"
        );
        assert_eq!(
            tm.corridor_at(0, 0),
            0.0,
            "non-chokepoint tile should have no corridor pressure"
        );
    }

    #[test]
    fn threat_map_exposure_is_threat_minus_defense() {
        let mut tm = ThreatMap::new(10, 10);
        // Set wolf territory at (3,3) and garrison coverage at (3,3)
        tm.wolf_territory[3 * 10 + 3] = 0.8;
        tm.garrison_coverage[3 * 10 + 3] = 0.5;
        // Set wolf territory at (7,7) with no garrison
        tm.wolf_territory[7 * 10 + 7] = 0.9;

        tm.recompute_exposure();

        let defended = tm.exposure_at(3, 3);
        let exposed = tm.exposure_at(7, 7);

        assert!(
            defended < exposed,
            "defended tile should have less exposure ({} vs {})",
            defended,
            exposed
        );
        assert!(
            (defended - 0.3).abs() < 0.01,
            "defended exposure should be 0.8 - 0.5 = 0.3, got {}",
            defended
        );
        assert!(
            (exposed - 0.9).abs() < 0.01,
            "exposed tile should be 0.9, got {}",
            exposed
        );
    }

    #[test]
    fn threat_map_exposure_clamped_to_zero() {
        let mut tm = ThreatMap::new(5, 5);
        // Garrison coverage exceeds threat
        tm.wolf_territory[12] = 0.2;
        tm.garrison_coverage[12] = 1.0;

        tm.recompute_exposure();

        assert_eq!(
            tm.exposure_at(2, 2),
            0.0,
            "exposure should clamp to 0 when defense exceeds threat"
        );
    }

    #[test]
    fn threat_map_out_of_bounds_returns_zero() {
        let tm = ThreatMap::new(5, 5);
        assert_eq!(tm.wolf_at(10, 10), 0.0);
        assert_eq!(tm.garrison_at(10, 10), 0.0);
        assert_eq!(tm.corridor_at(10, 10), 0.0);
        assert_eq!(tm.exposure_at(10, 10), 0.0);
    }

    // --- ExplorationMap tests ---

    #[test]
    fn exploration_starts_all_unrevealed() {
        let em = ExplorationMap::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                assert!(
                    !em.is_revealed(x, y),
                    "tile ({}, {}) should start unrevealed",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn exploration_reveal_marks_correct_tiles() {
        let mut em = ExplorationMap::new(32, 32);
        em.reveal(16, 16, 3);

        // Center should be revealed
        assert!(em.is_revealed(16, 16));
        // Tiles within radius 3
        assert!(em.is_revealed(16, 14)); // 2 tiles up
        assert!(em.is_revealed(18, 16)); // 2 tiles right
        assert!(em.is_revealed(14, 16)); // 2 tiles left
        assert!(em.is_revealed(16, 18)); // 2 tiles down

        // Tile at distance exactly 3 (on axis) should be revealed
        assert!(em.is_revealed(16, 13)); // 3 tiles up
        assert!(em.is_revealed(19, 16)); // 3 tiles right

        // Tile at distance > 3 should NOT be revealed
        assert!(!em.is_revealed(16, 12)); // 4 tiles up
        assert!(!em.is_revealed(20, 16)); // 4 tiles right

        // Far corner should not be revealed
        assert!(!em.is_revealed(0, 0));
        assert!(!em.is_revealed(31, 31));
    }

    #[test]
    fn exploration_reveal_near_edges() {
        let mut em = ExplorationMap::new(10, 10);
        // Reveal near corner — should not panic
        em.reveal(0, 0, 3);
        assert!(em.is_revealed(0, 0));
        assert!(em.is_revealed(2, 2));
        assert!(!em.is_revealed(4, 0)); // distance 4 > 3

        em.reveal(9, 9, 2);
        assert!(em.is_revealed(9, 9));
        assert!(em.is_revealed(8, 8));
    }

    #[test]
    fn exploration_is_revealed_out_of_bounds() {
        let em = ExplorationMap::new(10, 10);
        assert!(!em.is_revealed(100, 100));
        assert!(!em.is_revealed(10, 0));
        assert!(!em.is_revealed(0, 10));
    }

    #[test]
    fn exploration_multiple_reveals_accumulate() {
        let mut em = ExplorationMap::new(32, 32);
        em.reveal(5, 16, 2);
        em.reveal(25, 16, 2);

        // Both areas should be revealed
        assert!(em.is_revealed(5, 16));
        assert!(em.is_revealed(25, 16));
        // Gap between them should not be revealed
        assert!(!em.is_revealed(15, 16));
    }
}
