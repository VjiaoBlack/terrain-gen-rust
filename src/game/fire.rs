use rand::RngExt;

use crate::ecs::{Creature, Position, ProcessingBuilding, Recipe};
use crate::simulation::Season;
use crate::tilemap::Terrain;

impl super::Game {
    // --- Forest fire system ---

    /// Check for fire ignition. Called once per in-game day during summer
    /// when conditions are right: low moisture on flammable tiles.
    pub(super) fn check_fire_ignition(&mut self) {
        let season = self.day_night.season;
        // Fire only ignites in summer
        if season != Season::Summer {
            return;
        }

        let mut rng = rand::rng();
        let w = self.map.width;
        let h = self.map.height;

        // Sample up to 50 random tiles for lightning ignition
        let samples = 50usize.min(w * h);
        for _ in 0..samples {
            let x = rng.random_range(0..w as u32) as usize;
            let y = rng.random_range(0..h as u32) as usize;
            let Some(terrain) = self.map.get(x, y).copied() else {
                continue;
            };
            if !terrain.is_flammable() {
                continue;
            }
            let moisture = self.state.moisture.get(x, y);
            if moisture >= 0.15 {
                continue;
            }
            // 0.01% chance per eligible tile per day-tick (0.0001)
            if rng.random_range(0u32..10000) < 1 {
                self.ignite_tile(x, y, &mut rng);
                return; // At most one lightning ignition per day
            }
        }

        // Smithy/bakery building ignition: check tiles within 2 of each
        let building_positions: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &ProcessingBuilding)>()
            .iter()
            .filter(|(_, pb)| matches!(pb.recipe, Recipe::StoneToMasonry | Recipe::GrainToBread))
            .map(|(pos, _)| (pos.x, pos.y))
            .collect();

        for (bx, by) in building_positions {
            let ix = bx.round() as i32;
            let iy = by.round() as i32;
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let nx = ix + dx;
                    let ny = iy + dy;
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let ux = nx as usize;
                    let uy = ny as usize;
                    let Some(terrain) = self.map.get(ux, uy).copied() else {
                        continue;
                    };
                    if !terrain.is_flammable() {
                        continue;
                    }
                    let moisture = self.state.moisture.get(ux, uy);
                    if moisture >= 0.15 {
                        continue;
                    }
                    // 0.1% chance per tile per day (0.001)
                    if rng.random_range(0u32..1000) < 1 {
                        self.ignite_tile(ux, uy, &mut rng);
                        return;
                    }
                }
            }
        }
    }

    /// Ignite a single tile: set it to Burning, assign burn timer, add to fire_tiles.
    pub(super) fn ignite_tile(&mut self, x: usize, y: usize, rng: &mut impl rand::RngExt) {
        self.map.set(x, y, Terrain::Burning);
        self.dirty.mark(x, y);
        let burn_ticks = rng.random_range(30u32..=50);
        self.fire_tiles.push((x, y, burn_ticks));
        self.notify("Fire! A forest fire has started!".to_string());
    }

    /// Process fire spread and burnout each tick. Only iterates over active
    /// fire tiles -- O(fire_front), not O(map).
    pub(super) fn tick_fire(&mut self) {
        if self.fire_tiles.is_empty() {
            return;
        }

        let mut rng = rand::rng();
        let w = self.map.width;
        let h = self.map.height;
        let mut new_fires: Vec<(usize, usize, u32)> = Vec::new();

        // Build a set of currently burning positions for fast lookup
        let burning_set: std::collections::HashSet<(usize, usize)> =
            self.fire_tiles.iter().map(|&(x, y, _)| (x, y)).collect();

        // Decrement timers and collect spread candidates
        for entry in &mut self.fire_tiles {
            let (x, y, ref mut timer) = *entry;

            if *timer > 0 {
                *timer -= 1;
            }

            // Try to spread to 8 neighbors
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let ux = nx as usize;
                    let uy = ny as usize;
                    let Some(terrain) = self.map.get(ux, uy).copied() else {
                        continue;
                    };
                    if !terrain.is_flammable() {
                        continue;
                    }
                    // High moisture blocks spread
                    let moisture = self.state.moisture.get(ux, uy);
                    if moisture > 0.6 {
                        continue;
                    }
                    // spread_probability = 0.03 * (1.0 - moisture) * vegetation_factor
                    let veg = self.state.vegetation.get(ux, uy).clamp(0.3, 1.0);
                    let prob = 0.03 * (1.0 - moisture) * veg;
                    let roll = rng.random_range(0u32..10000) as f64 / 10000.0;
                    if roll < prob {
                        let already = burning_set.contains(&(ux, uy))
                            || new_fires.iter().any(|(fx, fy, _)| *fx == ux && *fy == uy);
                        if !already {
                            let burn_ticks = rng.random_range(30u32..=50);
                            new_fires.push((ux, uy, burn_ticks));
                        }
                    }
                }
            }
        }

        // Burnout: tiles whose timer hit 0 become Scorched
        let mut burned_out: Vec<(usize, usize)> = Vec::new();
        self.fire_tiles.retain(|&(x, y, timer)| {
            if timer == 0 {
                burned_out.push((x, y));
                false
            } else {
                true
            }
        });
        for (x, y) in &burned_out {
            self.map.set(*x, *y, Terrain::Scorched);
            self.dirty.mark(*x, *y);
            // Ash fertility bonus
            self.soil_fertility.add(*x, *y, 0.05);
            // Clear vegetation
            if let Some(v) = self.state.vegetation.get_mut(*x, *y) {
                *v = 0.0;
            }
        }

        // Set new fire tiles on the map and add to tracking list
        for &(x, y, _) in &new_fires {
            self.map.set(x, y, Terrain::Burning);
            self.dirty.mark(x, y);
        }
        self.fire_tiles.extend(new_fires);

        // Damage entities on burning tiles
        self.fire_damage_entities();
    }

    /// Entities standing on Burning tiles take hunger damage.
    pub(super) fn fire_damage_entities(&mut self) {
        let mut damage_targets: Vec<hecs::Entity> = Vec::new();
        for (entity, (pos, _creature)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Creature))>()
            .iter()
        {
            let tx = pos.x.round() as usize;
            let ty = pos.y.round() as usize;
            if self.map.get(tx, ty) == Some(&Terrain::Burning) {
                damage_targets.push(entity);
            }
        }
        for entity in damage_targets {
            if let Ok(mut creature) = self.world.get::<&mut Creature>(entity) {
                creature.hunger += 2.0;
            }
        }
    }

    /// Check if there are any burning tiles visible from a position.
    pub fn burning_tiles_near(&self, x: f64, y: f64, range: f64) -> Option<(f64, f64)> {
        if self.fire_tiles.is_empty() {
            return None;
        }
        let range_sq = range * range;
        let mut nearest_dist_sq = f64::INFINITY;
        let mut nearest = None;
        for &(fx, fy, _) in &self.fire_tiles {
            let dx = fx as f64 - x;
            let dy = fy as f64 - y;
            let d2 = dx * dx + dy * dy;
            if d2 < range_sq && d2 < nearest_dist_sq {
                nearest_dist_sq = d2;
                nearest = Some((fx as f64, fy as f64));
            }
        }
        nearest
    }
}
