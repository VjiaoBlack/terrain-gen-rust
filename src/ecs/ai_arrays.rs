//! Data-oriented parallel arrays for AI hot-path data.
//!
//! Extracts frequently-read creature fields from hecs into contiguous,
//! cache-friendly parallel arrays. Rebuilt from the World each tick before
//! system_ai runs. See docs/design/pillar5_scale/data_oriented_arrays.md.

use hecs::{Entity, World};

use super::components::{Behavior, BehaviorState, Creature, Position, Species};

/// Dense parallel arrays holding the AI-hot-path fields for every entity
/// that has Position + Creature + Behavior.  Indexed by a dense local
/// index `0..len`; the `entities` vec maps back to hecs Entity IDs.
///
/// Rebuilt every tick via [`AiArrays::extract`].  Read-only during the AI
/// loop; outputs are collected separately and written back via [`AiArrays::apply`].
pub struct AiArrays {
    pub len: usize,

    // --- Identity (for writeback) ---
    pub entities: Vec<Entity>,

    // --- Position ---
    pub x: Vec<f64>,
    pub y: Vec<f64>,

    // --- Creature fields ---
    pub species: Vec<Species>,
    pub hunger: Vec<f64>,
    pub sight_range: Vec<f64>,
    pub home_x: Vec<f64>,
    pub home_y: Vec<f64>,

    // --- Behavior fields ---
    pub state: Vec<BehaviorState>,
    pub speed: Vec<f64>,
}

impl AiArrays {
    /// Create with pre-allocated capacity.  Call once in `Game::new`.
    pub fn new(capacity: usize) -> Self {
        AiArrays {
            len: 0,
            entities: Vec::with_capacity(capacity),
            x: Vec::with_capacity(capacity),
            y: Vec::with_capacity(capacity),
            species: Vec::with_capacity(capacity),
            hunger: Vec::with_capacity(capacity),
            sight_range: Vec::with_capacity(capacity),
            home_x: Vec::with_capacity(capacity),
            home_y: Vec::with_capacity(capacity),
            state: Vec::with_capacity(capacity),
            speed: Vec::with_capacity(capacity),
        }
    }

    /// Clear all arrays but retain allocated capacity.
    pub fn clear(&mut self) {
        self.len = 0;
        self.entities.clear();
        self.x.clear();
        self.y.clear();
        self.species.clear();
        self.hunger.clear();
        self.sight_range.clear();
        self.home_x.clear();
        self.home_y.clear();
        self.state.clear();
        self.speed.clear();
    }

    /// Extract hot-path fields from hecs in a single query pass.
    ///
    /// One archetype traversal replaces the per-entity `world.get` calls
    /// that system_ai currently does.  The `&World` borrow is released
    /// before any mutation happens.
    pub fn extract(&mut self, world: &World) {
        self.clear();

        for (entity, pos, creature, behavior) in world
            .query::<(Entity, &Position, &Creature, &Behavior)>()
            .iter()
        {
            self.entities.push(entity);
            self.x.push(pos.x);
            self.y.push(pos.y);
            self.species.push(creature.species);
            self.hunger.push(creature.hunger);
            self.sight_range.push(creature.sight_range);
            self.home_x.push(creature.home_x);
            self.home_y.push(creature.home_y);
            self.state.push(behavior.state);
            self.speed.push(behavior.speed);
        }

        self.len = self.entities.len();
    }

    /// Write modified behavior states, velocities, and hunger back to hecs.
    ///
    /// Called once after the AI loop with collected outputs.  Each output
    /// corresponds to `self.entities[i]`.
    pub fn apply(&self, world: &mut World, outputs: &[AiOutput]) {
        for (i, output) in outputs.iter().enumerate() {
            let e = self.entities[i];
            if let Ok(mut behavior) = world.get::<&mut Behavior>(e) {
                behavior.state = output.new_state;
            }
            if let Ok(mut vel) = world.get::<&mut super::components::Velocity>(e) {
                vel.dx = output.new_vx;
                vel.dy = output.new_vy;
            }
            if let Ok(mut creature) = world.get::<&mut Creature>(e) {
                creature.hunger = output.new_hunger;
            }
        }
    }
}

/// Result of one entity's AI tick.  Only the fields that AI can change.
#[derive(Debug, Clone, Copy)]
pub struct AiOutput {
    pub new_state: BehaviorState,
    pub new_vx: f64,
    pub new_vy: f64,
    pub new_hunger: f64,
}

impl AiOutput {
    /// No-op output: entity keeps its current state and stops moving.
    pub fn noop(state: BehaviorState, hunger: f64) -> Self {
        AiOutput {
            new_state: state,
            new_vx: 0.0,
            new_vy: 0.0,
            new_hunger: hunger,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::{Behavior, BehaviorState, Creature, Position, Species, Velocity};
    use hecs::World;

    fn make_creature(world: &mut World, species: Species, x: f64, y: f64, hunger: f64) -> Entity {
        world.spawn((
            Position { x, y },
            Velocity { dx: 0.0, dy: 0.0 },
            Creature {
                species,
                hunger,
                home_x: 10.0,
                home_y: 10.0,
                sight_range: 8.0,
            },
            Behavior {
                state: BehaviorState::Idle { timer: 5 },
                speed: 1.0,
            },
        ))
    }

    #[test]
    fn extract_empty_world() {
        let world = World::new();
        let mut arrays = AiArrays::new(16);
        arrays.extract(&world);
        assert_eq!(arrays.len, 0);
        assert!(arrays.entities.is_empty());
    }

    #[test]
    fn extract_creatures_only() {
        let mut world = World::new();

        // Creature entity — should be extracted
        let e1 = make_creature(&mut world, Species::Prey, 5.0, 6.0, 0.3);

        // Non-creature entity (no Creature component) — should be skipped
        world.spawn((Position { x: 1.0, y: 1.0 }, Velocity { dx: 0.0, dy: 0.0 }));

        let mut arrays = AiArrays::new(16);
        arrays.extract(&world);

        assert_eq!(arrays.len, 1);
        assert_eq!(arrays.entities[0], e1);
        assert!((arrays.x[0] - 5.0).abs() < 1e-9);
        assert!((arrays.y[0] - 6.0).abs() < 1e-9);
        assert_eq!(arrays.species[0], Species::Prey);
        assert!((arrays.hunger[0] - 0.3).abs() < 1e-9);
        assert!((arrays.sight_range[0] - 8.0).abs() < 1e-9);
        assert!((arrays.home_x[0] - 10.0).abs() < 1e-9);
        assert!((arrays.home_y[0] - 10.0).abs() < 1e-9);
        assert!((arrays.speed[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn extract_multiple_species() {
        let mut world = World::new();
        make_creature(&mut world, Species::Prey, 1.0, 2.0, 0.1);
        make_creature(&mut world, Species::Predator, 3.0, 4.0, 0.5);
        make_creature(&mut world, Species::Villager, 5.0, 6.0, 0.0);

        let mut arrays = AiArrays::new(16);
        arrays.extract(&world);

        assert_eq!(arrays.len, 3);
        // All three species should be present (order depends on hecs internals)
        assert!(arrays.species.iter().any(|s| *s == Species::Prey));
        assert!(arrays.species.iter().any(|s| *s == Species::Predator));
        assert!(arrays.species.iter().any(|s| *s == Species::Villager));
    }

    #[test]
    fn clear_retains_capacity() {
        let mut world = World::new();
        for i in 0..20 {
            make_creature(&mut world, Species::Prey, i as f64, 0.0, 0.0);
        }

        let mut arrays = AiArrays::new(4);
        arrays.extract(&world);
        assert_eq!(arrays.len, 20);

        let cap_before = arrays.x.capacity();
        assert!(cap_before >= 20);

        arrays.clear();
        assert_eq!(arrays.len, 0);
        assert!(arrays.entities.is_empty());
        // Capacity retained
        assert_eq!(arrays.x.capacity(), cap_before);
    }

    #[test]
    fn extract_twice_overwrites() {
        let mut world = World::new();
        make_creature(&mut world, Species::Prey, 1.0, 2.0, 0.1);

        let mut arrays = AiArrays::new(16);
        arrays.extract(&world);
        assert_eq!(arrays.len, 1);

        // Add more entities, re-extract
        make_creature(&mut world, Species::Predator, 3.0, 4.0, 0.5);
        arrays.extract(&world);
        assert_eq!(arrays.len, 2);
    }

    #[test]
    fn apply_writes_back() {
        let mut world = World::new();
        let e = make_creature(&mut world, Species::Villager, 5.0, 5.0, 0.4);

        let mut arrays = AiArrays::new(16);
        arrays.extract(&world);

        let outputs = vec![AiOutput {
            new_state: BehaviorState::Wander { timer: 10 },
            new_vx: 1.5,
            new_vy: -0.5,
            new_hunger: 0.6,
        }];

        arrays.apply(&mut world, &outputs);

        let behavior = world.get::<&Behavior>(e).unwrap();
        assert!(matches!(
            behavior.state,
            BehaviorState::Wander { timer: 10 }
        ));

        let vel = world.get::<&Velocity>(e).unwrap();
        assert!((vel.dx - 1.5).abs() < 1e-9);
        assert!((vel.dy - (-0.5)).abs() < 1e-9);

        let creature = world.get::<&Creature>(e).unwrap();
        assert!((creature.hunger - 0.6).abs() < 1e-9);
    }

    #[test]
    fn noop_output() {
        let state = BehaviorState::Idle { timer: 3 };
        let out = AiOutput::noop(state, 0.25);
        assert!((out.new_vx).abs() < 1e-9);
        assert!((out.new_vy).abs() < 1e-9);
        assert!((out.new_hunger - 0.25).abs() < 1e-9);
    }
}
