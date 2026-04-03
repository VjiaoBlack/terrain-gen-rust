use hecs::{Entity, World};

use super::{
    BuildSite, Creature, FoodSource, HutBuilding, Position, Species, Stockpile, StoneDeposit,
};

// --- Category bitflags ---

pub mod category {
    pub const VILLAGER: u16 = 1 << 0;
    pub const PREDATOR: u16 = 1 << 1;
    pub const PREY: u16 = 1 << 2;
    pub const FOOD_SOURCE: u16 = 1 << 3;
    pub const STOCKPILE: u16 = 1 << 4;
    pub const BUILD_SITE: u16 = 1 << 5;
    pub const STONE_DEPOSIT: u16 = 1 << 6;
    pub const HUT: u16 = 1 << 7;
    pub const BUILDING: u16 = 1 << 8;
}

/// An entry in the spatial grid.
#[derive(Clone, Copy, Debug)]
pub struct SpatialEntry {
    pub entity: Entity,
    pub x: f64,
    pub y: f64,
    pub categories: u16,
}

/// Fixed-grid spatial partitioning for O(nearby) entity lookups.
pub struct SpatialHashGrid {
    cell_size: usize,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<SpatialEntry>>,
}

impl SpatialHashGrid {
    pub fn new(map_width: usize, map_height: usize, cell_size: usize) -> Self {
        let cols = (map_width + cell_size - 1) / cell_size;
        let rows = (map_height + cell_size - 1) / cell_size;
        SpatialHashGrid {
            cell_size,
            cols,
            rows,
            cells: (0..cols * rows).map(|_| Vec::with_capacity(16)).collect(),
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn insert(&mut self, entry: SpatialEntry) {
        let cx = (entry.x as usize) / self.cell_size;
        let cy = (entry.y as usize) / self.cell_size;
        if cx < self.cols && cy < self.rows {
            self.cells[cy * self.cols + cx].push(entry);
        }
    }

    /// Iterate all entries within `radius` of `(cx, cy)` matching `category_mask`.
    pub fn query_radius(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        category_mask: u16,
    ) -> impl Iterator<Item = &SpatialEntry> {
        let min_col = ((cx - radius).max(0.0) as usize) / self.cell_size;
        let max_col = ((cx + radius) as usize / self.cell_size).min(self.cols.saturating_sub(1));
        let min_row = ((cy - radius).max(0.0) as usize) / self.cell_size;
        let max_row = ((cy + radius) as usize / self.cell_size).min(self.rows.saturating_sub(1));
        let r_sq = radius * radius;

        (min_row..=max_row)
            .flat_map(move |row| {
                (min_col..=max_col).flat_map(move |col| self.cells[row * self.cols + col].iter())
            })
            .filter(move |e| e.categories & category_mask != 0)
            .filter(move |e| {
                let dx = e.x - cx;
                let dy = e.y - cy;
                dx * dx + dy * dy <= r_sq
            })
    }

    /// Find the single nearest entry matching `category_mask` within `radius`.
    pub fn nearest(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        category_mask: u16,
    ) -> Option<(SpatialEntry, f64)> {
        let mut best: Option<(SpatialEntry, f64)> = None;
        for entry in self.query_radius(cx, cy, radius, category_mask) {
            let dx = entry.x - cx;
            let dy = entry.y - cy;
            let d_sq = dx * dx + dy * dy;
            if best.is_none() || d_sq < best.unwrap().1 {
                best = Some((*entry, d_sq));
            }
        }
        best.map(|(e, d_sq)| (e, d_sq.sqrt()))
    }

    /// Check if ANY entry matching `category_mask` exists within `radius`.
    pub fn any_within(&self, cx: f64, cy: f64, radius: f64, category_mask: u16) -> bool {
        self.query_radius(cx, cy, radius, category_mask)
            .next()
            .is_some()
    }

    /// Count entries matching `category_mask` within `radius`.
    pub fn count_within(&self, cx: f64, cy: f64, radius: f64, category_mask: u16) -> usize {
        self.query_radius(cx, cy, radius, category_mask).count()
    }

    /// Get all entries in a specific cell (for rendering).
    pub fn entries_in_cell(&self, cell_x: usize, cell_y: usize) -> &[SpatialEntry] {
        if cell_x < self.cols && cell_y < self.rows {
            &self.cells[cell_y * self.cols + cell_x]
        } else {
            &[]
        }
    }

    /// Collect all entries matching `category_mask` across the entire grid.
    pub fn all_of_category(&self, category_mask: u16) -> Vec<SpatialEntry> {
        self.cells
            .iter()
            .flat_map(|cell| cell.iter())
            .filter(|e| e.categories & category_mask != 0)
            .copied()
            .collect()
    }

    /// Populate the grid from the hecs World in a single pass.
    pub fn populate(&mut self, world: &World) {
        self.clear();
        for (entity, pos) in world.query::<(Entity, &Position)>().iter() {
            let mut cats: u16 = 0;

            if let Ok(creature) = world.get::<&Creature>(entity) {
                match creature.species {
                    Species::Villager => cats |= category::VILLAGER,
                    Species::Predator => cats |= category::PREDATOR,
                    Species::Prey => cats |= category::PREY,
                }
            }
            if world.get::<&FoodSource>(entity).is_ok() {
                cats |= category::FOOD_SOURCE;
            }
            if world.get::<&Stockpile>(entity).is_ok() {
                cats |= category::STOCKPILE;
            }
            if world.get::<&BuildSite>(entity).is_ok() {
                cats |= category::BUILD_SITE;
            }
            if world.get::<&StoneDeposit>(entity).is_ok() {
                cats |= category::STONE_DEPOSIT;
            }
            if world.get::<&HutBuilding>(entity).is_ok() {
                cats |= category::HUT;
            }

            self.insert(SpatialEntry {
                entity,
                x: pos.x,
                y: pos.y,
                categories: cats,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(x: f64, y: f64, cats: u16) -> SpatialEntry {
        // Create a dummy entity for testing
        let mut world = World::new();
        let e = world.spawn(());
        SpatialEntry {
            entity: e,
            x,
            y,
            categories: cats,
        }
    }

    #[test]
    fn empty_grid_returns_nothing() {
        let grid = SpatialHashGrid::new(256, 256, 16);
        assert!(
            grid.nearest(128.0, 128.0, 50.0, category::VILLAGER)
                .is_none()
        );
        assert!(!grid.any_within(128.0, 128.0, 50.0, category::VILLAGER));
        assert_eq!(grid.count_within(128.0, 128.0, 50.0, category::VILLAGER), 0);
    }

    #[test]
    fn insert_and_find_by_category() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(100.0, 100.0, category::VILLAGER));
        grid.insert(make_entry(105.0, 100.0, category::FOOD_SOURCE));

        assert!(
            grid.nearest(100.0, 100.0, 10.0, category::VILLAGER)
                .is_some()
        );
        assert!(
            grid.nearest(100.0, 100.0, 10.0, category::FOOD_SOURCE)
                .is_some()
        );
        assert!(
            grid.nearest(100.0, 100.0, 10.0, category::PREDATOR)
                .is_none()
        );
    }

    #[test]
    fn outside_radius_not_found() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(100.0, 100.0, category::VILLAGER));

        assert!(
            grid.nearest(200.0, 200.0, 10.0, category::VILLAGER)
                .is_none()
        );
        assert!(
            grid.nearest(100.0, 100.0, 5.0, category::VILLAGER)
                .is_some()
        );
    }

    #[test]
    fn nearest_returns_closest() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(110.0, 100.0, category::FOOD_SOURCE));
        grid.insert(make_entry(105.0, 100.0, category::FOOD_SOURCE));
        grid.insert(make_entry(102.0, 100.0, category::FOOD_SOURCE));

        let (entry, dist) = grid
            .nearest(101.0, 100.0, 20.0, category::FOOD_SOURCE)
            .unwrap();
        assert!(
            (entry.x - 102.0).abs() < 0.01,
            "should find the closest one at 102, got {}",
            entry.x
        );
        assert!((dist - 1.0).abs() < 0.01);
    }

    #[test]
    fn any_within_short_circuits() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(50.0, 50.0, category::PREDATOR));

        assert!(grid.any_within(50.0, 50.0, 5.0, category::PREDATOR));
        assert!(!grid.any_within(200.0, 200.0, 5.0, category::PREDATOR));
    }

    #[test]
    fn count_within_accurate() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        for i in 0..5 {
            grid.insert(make_entry(100.0 + i as f64, 100.0, category::VILLAGER));
        }
        grid.insert(make_entry(200.0, 200.0, category::VILLAGER));

        assert_eq!(grid.count_within(102.0, 100.0, 10.0, category::VILLAGER), 5);
        assert_eq!(grid.count_within(200.0, 200.0, 5.0, category::VILLAGER), 1);
    }

    #[test]
    fn entities_at_map_edges() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(0.0, 0.0, category::VILLAGER));
        grid.insert(make_entry(255.0, 255.0, category::VILLAGER));

        assert!(grid.nearest(0.0, 0.0, 5.0, category::VILLAGER).is_some());
        assert!(
            grid.nearest(255.0, 255.0, 5.0, category::VILLAGER)
                .is_some()
        );
    }

    #[test]
    fn clear_and_rebuild() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(100.0, 100.0, category::VILLAGER));
        assert_eq!(grid.count_within(100.0, 100.0, 10.0, category::VILLAGER), 1);

        grid.clear();
        assert_eq!(grid.count_within(100.0, 100.0, 10.0, category::VILLAGER), 0);

        grid.insert(make_entry(50.0, 50.0, category::PREDATOR));
        assert_eq!(grid.count_within(50.0, 50.0, 10.0, category::PREDATOR), 1);
    }

    #[test]
    fn multi_category_entity() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(
            100.0,
            100.0,
            category::FOOD_SOURCE | category::BUILDING,
        ));

        assert!(
            grid.nearest(100.0, 100.0, 10.0, category::FOOD_SOURCE)
                .is_some()
        );
        assert!(
            grid.nearest(100.0, 100.0, 10.0, category::BUILDING)
                .is_some()
        );
        assert!(
            grid.nearest(100.0, 100.0, 10.0, category::VILLAGER)
                .is_none()
        );
    }

    #[test]
    fn cross_cell_boundary_query() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        // Place entity at cell boundary (cell 0 ends at x=16, cell 1 starts at x=16)
        grid.insert(make_entry(15.0, 15.0, category::VILLAGER));

        // Query from neighboring cell should find it
        assert!(grid.nearest(17.0, 15.0, 5.0, category::VILLAGER).is_some());
    }

    // --- New hardening tests ---

    #[test]
    fn radius_zero_matches_exact_position_only() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(50.0, 50.0, category::VILLAGER));
        grid.insert(make_entry(50.5, 50.0, category::VILLAGER));

        // Radius 0 should only match entities at distance 0 (i.e., exact position)
        let count = grid.count_within(50.0, 50.0, 0.0, category::VILLAGER);
        assert_eq!(
            count, 1,
            "radius=0 should only match entity at exact position"
        );
    }

    #[test]
    fn large_radius_covers_entire_map() {
        let mut grid = SpatialHashGrid::new(64, 64, 16);
        grid.insert(make_entry(0.0, 0.0, category::VILLAGER));
        grid.insert(make_entry(63.0, 63.0, category::PREDATOR));
        grid.insert(make_entry(32.0, 32.0, category::FOOD_SOURCE));

        // Query from center with radius large enough to cover entire 64x64 map
        let count = grid.count_within(
            32.0,
            32.0,
            100.0,
            category::VILLAGER | category::PREDATOR | category::FOOD_SOURCE,
        );
        assert_eq!(count, 3, "large radius should find all entities on map");
    }

    #[test]
    fn fractional_coordinates() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(0.5, 0.5, category::VILLAGER));

        assert!(
            grid.nearest(0.5, 0.5, 1.0, category::VILLAGER).is_some(),
            "should find entity at fractional coordinates"
        );
        let (entry, dist) = grid.nearest(0.5, 0.5, 1.0, category::VILLAGER).unwrap();
        assert!((entry.x - 0.5).abs() < 0.001);
        assert!((entry.y - 0.5).abs() < 0.001);
        assert!(dist < 0.001, "distance to self should be ~0, got {}", dist);
    }

    #[test]
    fn multiple_entities_same_tile() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.insert(make_entry(50.0, 50.0, category::VILLAGER));
        grid.insert(make_entry(50.0, 50.0, category::VILLAGER));
        grid.insert(make_entry(50.0, 50.0, category::PREDATOR));

        assert_eq!(
            grid.count_within(50.0, 50.0, 1.0, category::VILLAGER),
            2,
            "should find both villagers on same tile"
        );
        assert_eq!(
            grid.count_within(50.0, 50.0, 1.0, category::PREDATOR),
            1,
            "should find the predator on same tile"
        );
        assert_eq!(
            grid.count_within(50.0, 50.0, 1.0, category::VILLAGER | category::PREDATOR),
            3,
            "combined mask should find all 3"
        );
    }

    #[test]
    fn populate_from_world_with_mixed_entity_types() {
        let mut world = World::new();
        use super::{Creature, FoodSource, Position, Species, Stockpile, StoneDeposit};

        // Spawn a villager
        world.spawn((
            Position { x: 10.0, y: 10.0 },
            Creature {
                species: Species::Villager,
                hunger: 0.0,
                home_x: 10.0,
                home_y: 10.0,
                sight_range: 12.0,
            },
        ));
        // Spawn a predator
        world.spawn((
            Position { x: 20.0, y: 20.0 },
            Creature {
                species: Species::Predator,
                hunger: 0.0,
                home_x: 20.0,
                home_y: 20.0,
                sight_range: 15.0,
            },
        ));
        // Spawn a food source
        world.spawn((Position { x: 30.0, y: 30.0 }, FoodSource));
        // Spawn a stockpile
        world.spawn((Position { x: 40.0, y: 40.0 }, Stockpile));
        // Spawn a stone deposit
        world.spawn((Position { x: 50.0, y: 50.0 }, StoneDeposit));

        let mut grid = SpatialHashGrid::new(64, 64, 16);
        grid.populate(&world);

        assert_eq!(
            grid.all_of_category(category::VILLAGER).len(),
            1,
            "should find 1 villager"
        );
        assert_eq!(
            grid.all_of_category(category::PREDATOR).len(),
            1,
            "should find 1 predator"
        );
        assert_eq!(
            grid.all_of_category(category::FOOD_SOURCE).len(),
            1,
            "should find 1 food source"
        );
        assert_eq!(
            grid.all_of_category(category::STOCKPILE).len(),
            1,
            "should find 1 stockpile"
        );
        assert_eq!(
            grid.all_of_category(category::STONE_DEPOSIT).len(),
            1,
            "should find 1 stone deposit"
        );
    }

    #[test]
    fn entries_in_cell_returns_correct_entities() {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        // Cell (0,0) covers x=0..16, y=0..16
        grid.insert(make_entry(5.0, 5.0, category::VILLAGER));
        grid.insert(make_entry(10.0, 10.0, category::PREDATOR));
        // Cell (1,0) covers x=16..32
        grid.insert(make_entry(20.0, 5.0, category::FOOD_SOURCE));

        assert_eq!(grid.entries_in_cell(0, 0).len(), 2);
        assert_eq!(grid.entries_in_cell(1, 0).len(), 1);
        assert_eq!(grid.entries_in_cell(2, 2).len(), 0);
    }

    #[test]
    fn out_of_bounds_entries_in_cell() {
        let grid = SpatialHashGrid::new(256, 256, 16);
        assert_eq!(
            grid.entries_in_cell(999, 999).len(),
            0,
            "out-of-bounds cell should return empty slice"
        );
    }
}
