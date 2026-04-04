use serde::{Deserialize, Serialize};

use crate::ecs::components::ResourceType;

/// Tracks accumulated foot traffic from villager movement.
/// High-traffic walkable tiles automatically convert to roads.
///
/// Extended with directional tracking (`traffic_dx`/`traffic_dy`) to orient
/// trail characters, and per-tile dominant resource type for the Traffic overlay.
#[derive(Serialize, Deserialize)]
pub struct TrafficMap {
    pub width: usize,
    pub height: usize,
    traffic: Vec<f64>,
    /// Accumulated movement direction X component per tile (Phase 2: directional trails).
    #[serde(default)]
    traffic_dx: Vec<f64>,
    /// Accumulated movement direction Y component per tile (Phase 2: directional trails).
    #[serde(default)]
    traffic_dy: Vec<f64>,
    /// Per-tile dominant resource type carried by haulers traversing the tile.
    #[serde(default)]
    dominant_resource: Vec<Option<ResourceType>>,
    /// Per-tile resource flow counters: [Food, Wood, Stone, Planks, Masonry, Grain].
    #[serde(default)]
    flow_by_type: Vec<[f64; 6]>,
}

/// Map a `ResourceType` to an index into `flow_by_type` arrays.
fn resource_type_index(rt: ResourceType) -> usize {
    match rt {
        ResourceType::Food => 0,
        ResourceType::Wood => 1,
        ResourceType::Stone => 2,
        ResourceType::Planks => 3,
        ResourceType::Masonry => 4,
        ResourceType::Grain => 5,
    }
}

impl TrafficMap {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            width,
            height,
            traffic: vec![0.0; n],
            traffic_dx: vec![0.0; n],
            traffic_dy: vec![0.0; n],
            dominant_resource: vec![None; n],
            flow_by_type: vec![[0.0; 6]; n],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.traffic[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Get the accumulated directional vector at a tile.
    /// Returns `(dx, dy)` where the magnitude reflects total directed traffic.
    pub fn get_direction(&self, x: usize, y: usize) -> (f64, f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if idx < self.traffic_dx.len() && idx < self.traffic_dy.len() {
                (self.traffic_dx[idx], self.traffic_dy[idx])
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    }

    /// Get the dominant resource type hauled across a tile, if any.
    pub fn get_dominant_resource(&self, x: usize, y: usize) -> Option<ResourceType> {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if idx < self.dominant_resource.len() {
                self.dominant_resource[idx]
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Record a footstep at the given position.
    pub fn step_on(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            self.traffic[y * self.width + x] += 1.0;
        }
    }

    /// Record a directed footstep with movement direction and optional resource cargo.
    /// `dx`/`dy` are the villager's velocity direction (will be normalized).
    /// Hauling steps get a 2x weight in the directional accumulator so net flow
    /// points toward stockpiles.
    pub fn step_on_directed(
        &mut self,
        x: usize,
        y: usize,
        dx: f64,
        dy: f64,
        resource: Option<ResourceType>,
    ) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.traffic[idx] += 1.0;

            // Normalize direction to unit length before accumulating
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.001 {
                let weight = if resource.is_some() { 2.0 } else { 1.0 };
                if idx < self.traffic_dx.len() {
                    self.traffic_dx[idx] += (dx / len) * weight;
                }
                if idx < self.traffic_dy.len() {
                    self.traffic_dy[idx] += (dy / len) * weight;
                }
            }

            // Track resource flow
            if let Some(rt) = resource {
                if idx < self.flow_by_type.len() {
                    self.flow_by_type[idx][resource_type_index(rt)] += 1.0;
                    // Recompute dominant resource for this tile
                    let counts = &self.flow_by_type[idx];
                    let mut best_idx = 0;
                    let mut best_val = counts[0];
                    for i in 1..6 {
                        if counts[i] > best_val {
                            best_val = counts[i];
                            best_idx = i;
                        }
                    }
                    if idx < self.dominant_resource.len() {
                        self.dominant_resource[idx] = if best_val > 0.0 {
                            Some(match best_idx {
                                0 => ResourceType::Food,
                                1 => ResourceType::Wood,
                                2 => ResourceType::Stone,
                                3 => ResourceType::Planks,
                                4 => ResourceType::Masonry,
                                _ => ResourceType::Grain,
                            })
                        } else {
                            None
                        };
                    }
                }
            }
        }
    }

    /// Slow decay so old paths fade if villagers stop using them.
    pub fn decay(&mut self) {
        for v in self.traffic.iter_mut() {
            *v *= 0.999;
        }
        for v in self.traffic_dx.iter_mut() {
            *v *= 0.999;
        }
        for v in self.traffic_dy.iter_mut() {
            *v *= 0.999;
        }
        for arr in self.flow_by_type.iter_mut() {
            for v in arr.iter_mut() {
                *v *= 0.999;
            }
        }
    }

    /// Return tiles that exceed the road threshold and are eligible for conversion.
    /// Only converts walkable non-road terrain (grass, sand, forest, building floor).
    pub fn road_candidates(
        &self,
        map: &crate::tilemap::TileMap,
        threshold: f64,
    ) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.traffic[y * self.width + x] >= threshold
                    && let Some(terrain) = map.get(x, y)
                    && terrain.is_walkable()
                    && *terrain != crate::tilemap::Terrain::Road
                    && *terrain != crate::tilemap::Terrain::BuildingFloor
                    && *terrain != crate::tilemap::Terrain::BuildingWall
                {
                    result.push((x, y));
                }
            }
        }
        result
    }

    /// Compute the dominant travel direction character for a trail-tier tile.
    /// Returns a trail character oriented along the dominant direction of travel.
    pub fn trail_char(&self, x: usize, y: usize) -> char {
        let (dx, dy) = self.get_direction(x, y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return '.'; // mixed / no dominant direction
        }
        // Compute angle and pick oriented character
        let angle = dy.atan2(dx).abs(); // 0 = east, pi/2 = south, pi = west
        if angle < std::f64::consts::FRAC_PI_8 || angle > 7.0 * std::f64::consts::FRAC_PI_8 {
            '-' // east-west
        } else if angle < 3.0 * std::f64::consts::FRAC_PI_8 {
            if (dx > 0.0) == (dy > 0.0) { '\\' } else { '/' }
        } else if angle < 5.0 * std::f64::consts::FRAC_PI_8 {
            '|' // north-south
        } else {
            if (dx > 0.0) == (dy > 0.0) { '/' } else { '\\' }
        }
    }
}

impl Default for TrafficMap {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilemap::{Terrain, TileMap};

    #[test]
    fn traffic_map_accumulates() {
        let mut tm = TrafficMap::new(10, 10);
        assert_eq!(tm.get(5, 5), 0.0);
        tm.step_on(5, 5);
        tm.step_on(5, 5);
        tm.step_on(5, 5);
        assert_eq!(tm.get(5, 5), 3.0);
    }

    #[test]
    fn traffic_map_decay() {
        let mut tm = TrafficMap::new(10, 10);
        for _ in 0..100 {
            tm.step_on(3, 3);
        }
        let before = tm.get(3, 3);
        for _ in 0..1000 {
            tm.decay();
        }
        let after = tm.get(3, 3);
        assert!(
            after < before * 0.5,
            "traffic should decay over time: {} -> {}",
            before,
            after
        );
    }

    #[test]
    fn traffic_road_candidates_only_walkable() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(2, 2, Terrain::BuildingWall); // unwalkable
        map.set(3, 3, Terrain::Road); // already road

        let mut tm = TrafficMap::new(10, 10);
        // Accumulate traffic on grass, wall, and road tiles
        for _ in 0..200 {
            tm.step_on(1, 1); // grass — should be candidate
            tm.step_on(2, 2); // wall — should NOT
            tm.step_on(3, 3); // road — should NOT
        }

        let candidates = tm.road_candidates(&map, 100.0);
        assert!(
            candidates.contains(&(1, 1)),
            "grass tile with high traffic should be candidate"
        );
        assert!(
            !candidates.contains(&(2, 2)),
            "wall tile should not be candidate"
        );
        assert!(
            !candidates.contains(&(3, 3)),
            "existing road should not be candidate"
        );
    }

    #[test]
    fn traffic_below_threshold_no_candidates() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let mut tm = TrafficMap::new(10, 10);
        tm.step_on(5, 5);
        tm.step_on(5, 5);

        let candidates = tm.road_candidates(&map, 100.0);
        assert!(
            candidates.is_empty(),
            "low traffic should not produce road candidates"
        );
    }

    // --- TrafficMap directional + resource flow tests ---

    #[test]
    fn traffic_step_on_directed_accumulates_direction() {
        let mut tm = TrafficMap::new(10, 10);
        // Walk eastward several times
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        assert_eq!(tm.get(5, 5), 3.0);
        let (dx, dy) = tm.get_direction(5, 5);
        assert!(dx > 0.0, "dx should be positive for eastward steps: {}", dx);
        assert!(
            dy.abs() < 0.001,
            "dy should be near zero for pure eastward: {}",
            dy
        );
    }

    #[test]
    fn traffic_step_on_directed_hauling_has_double_weight() {
        let mut tm = TrafficMap::new(10, 10);
        // One non-hauling step east
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        let (dx1, _) = tm.get_direction(5, 5);
        // One hauling step west (resource = Some)
        tm.step_on_directed(5, 5, -1.0, 0.0, Some(ResourceType::Wood));
        let (dx2, _) = tm.get_direction(5, 5);
        // Hauling west with 2x weight should dominate: 1.0 + (-2.0) = -1.0
        assert!(
            dx2 < 0.0,
            "net direction should be westward (hauling dominates): {}",
            dx2
        );
    }

    #[test]
    fn traffic_dominant_resource_tracks_most_hauled() {
        let mut tm = TrafficMap::new(10, 10);
        // 5 wood hauls
        for _ in 0..5 {
            tm.step_on_directed(3, 3, 1.0, 0.0, Some(ResourceType::Wood));
        }
        // 2 stone hauls
        for _ in 0..2 {
            tm.step_on_directed(3, 3, 1.0, 0.0, Some(ResourceType::Stone));
        }
        assert_eq!(
            tm.get_dominant_resource(3, 3),
            Some(ResourceType::Wood),
            "wood should dominate with 5 vs 2 hauls"
        );
    }

    #[test]
    fn traffic_dominant_resource_none_without_hauls() {
        let mut tm = TrafficMap::new(10, 10);
        tm.step_on_directed(3, 3, 1.0, 0.0, None);
        tm.step_on_directed(3, 3, 1.0, 0.0, None);
        assert_eq!(
            tm.get_dominant_resource(3, 3),
            None,
            "no hauls should mean no dominant resource"
        );
    }

    #[test]
    fn traffic_trail_char_horizontal() {
        let mut tm = TrafficMap::new(10, 10);
        // Strong eastward direction
        for _ in 0..20 {
            tm.step_on_directed(5, 5, 1.0, 0.0, None);
        }
        let ch = tm.trail_char(5, 5);
        assert_eq!(ch, '-', "horizontal traffic should produce '-' trail");
    }

    #[test]
    fn traffic_trail_char_vertical() {
        let mut tm = TrafficMap::new(10, 10);
        // Strong southward direction
        for _ in 0..20 {
            tm.step_on_directed(5, 5, 0.0, 1.0, None);
        }
        let ch = tm.trail_char(5, 5);
        assert_eq!(ch, '|', "vertical traffic should produce '|' trail");
    }

    #[test]
    fn traffic_trail_char_mixed_returns_dot() {
        let mut tm = TrafficMap::new(10, 10);
        // Exactly opposing directions cancel out
        tm.step_on_directed(5, 5, 1.0, 0.0, None);
        tm.step_on_directed(5, 5, -1.0, 0.0, None);
        let ch = tm.trail_char(5, 5);
        assert_eq!(ch, '.', "cancelled directions should produce '.' trail");
    }

    #[test]
    fn traffic_decay_affects_directional_and_flow() {
        let mut tm = TrafficMap::new(10, 10);
        for _ in 0..100 {
            tm.step_on_directed(3, 3, 1.0, 0.0, Some(ResourceType::Food));
        }
        let (dx_before, _) = tm.get_direction(3, 3);
        assert!(dx_before > 0.0);

        for _ in 0..1000 {
            tm.decay();
        }

        let (dx_after, _) = tm.get_direction(3, 3);
        assert!(
            dx_after < dx_before * 0.5,
            "directional accumulator should decay: {} -> {}",
            dx_before,
            dx_after
        );
    }

    #[test]
    fn traffic_get_direction_out_of_bounds() {
        let tm = TrafficMap::new(5, 5);
        let (dx, dy) = tm.get_direction(10, 10);
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn traffic_get_dominant_resource_out_of_bounds() {
        let tm = TrafficMap::new(5, 5);
        assert_eq!(tm.get_dominant_resource(10, 10), None);
    }
}
