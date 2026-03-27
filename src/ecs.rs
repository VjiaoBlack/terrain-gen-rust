use hecs::{Entity, World};
use serde::Serialize;

use crate::renderer::{Color, Renderer};
use crate::tilemap::TileMap;

// --- Components ---

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Velocity {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Sprite {
    pub ch: char,
    pub fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Species {
    Prey,
    Predator,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum BehaviorState {
    /// Wander randomly. Timer counts down to next direction change.
    Wander { timer: u32 },
    /// Move toward a target position.
    Seek { target_x: f64, target_y: f64 },
    /// Stand still. Timer counts down before switching to Wander.
    Idle { timer: u32 },
    /// Prey: eating at a food source.
    Eating { timer: u32 },
    /// Prey: fleeing home because predator is nearby.
    FleeHome,
    /// Prey: safe at home, resting until hungry.
    AtHome { timer: u32 },
    /// Predator: chasing a prey it spotted.
    Hunting { target_x: f64, target_y: f64 },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Behavior {
    pub state: BehaviorState,
    pub speed: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Creature {
    pub species: Species,
    pub hunger: f64,       // 0.0 = full, 1.0 = starving
    pub home_x: f64,
    pub home_y: f64,
    pub sight_range: f64,  // how far this creature can see
}

/// Marker component for berry bushes (food source for prey).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FoodSource;

/// Marker component for dens (safe home for prey).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Den;

// --- Systems ---

/// Move entities with terrain collision. Each axis is tested independently so
/// entities slide along walls. If blocked, velocity on that axis is reversed
/// (NPCs bounce).
pub fn system_movement(world: &mut World, map: &TileMap) {
    for (pos, vel) in world.query_mut::<(&mut Position, &mut Velocity)>() {
        // Try X
        let new_x = pos.x + vel.dx;
        if map.is_walkable(new_x, pos.y) {
            pos.x = new_x;
        } else {
            vel.dx = -vel.dx; // bounce
        }
        // Try Y
        let new_y = pos.y + vel.dy;
        if map.is_walkable(pos.x, new_y) {
            pos.y = new_y;
        } else {
            vel.dy = -vel.dy; // bounce
        }
    }
}


fn dist(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

fn move_toward(pos: &Position, tx: f64, ty: f64, speed: f64, vel: &mut Velocity) -> f64 {
    let dx = tx - pos.x;
    let dy = ty - pos.y;
    let d = dist(pos.x, pos.y, tx, ty);
    if d > 0.1 {
        vel.dx = (dx / d) * speed;
        vel.dy = (dy / d) * speed;
    }
    d
}

fn wander(pos: &Position, vel: &mut Velocity, speed: f64, map: &TileMap, rng: &mut impl rand::RngExt) {
    const DIRS: [(f64, f64); 8] = [
        (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
        (1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0),
    ];
    let mut candidates: Vec<(f64, f64)> = Vec::new();
    for &(dx, dy) in &DIRS {
        if map.is_walkable(pos.x + dx * 2.0, pos.y + dy * 2.0) {
            candidates.push((dx, dy));
        }
    }
    if let Some(&(dx, dy)) = candidates.get(rng.random_range(0..candidates.len().max(1))) {
        let len: f64 = (dx * dx + dy * dy).sqrt();
        vel.dx = dx / len * speed;
        vel.dy = dy / len * speed;
    } else {
        vel.dx = 0.0;
        vel.dy = 0.0;
    }
}

/// Hunger increases each tick.
/// Rate: 0.0005/tick → full hunger in ~2000 ticks (~1.7 in-game days at 0.02h/tick).
/// Creatures should eat roughly once per day.
pub fn system_hunger(world: &mut World) {
    for creature in world.query_mut::<&mut Creature>() {
        let rate = match creature.species {
            Species::Prey => 0.0005,
            Species::Predator => 0.0006, // predators burn slightly more
        };
        creature.hunger = (creature.hunger + rate).min(1.0);
    }
}

/// AI system: updates velocity based on behavior, species, and world state.
pub fn system_ai(world: &mut World, map: &TileMap) {
    let mut rng = rand::rng();

    // Phase 1: snapshot world state (positions of food, prey, predators)
    let food_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &FoodSource)>()
        .iter()
        .map(|(pos, _)| (pos.x, pos.y))
        .collect();

    let prey_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
        .filter(|(_, _, c, _)| c.species == Species::Prey)
        .map(|(e, p, _, b)| (e, p.x, p.y, matches!(b.state, BehaviorState::AtHome { .. })))
        .collect();

    let predator_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &Creature)>()
        .iter()
        .filter(|(_, c)| c.species == Species::Predator)
        .map(|(p, _)| (p.x, p.y))
        .collect();

    // Phase 2: collect entity IDs with Behavior
    let entities: Vec<Entity> = world
        .query::<(Entity, &Behavior)>()
        .iter()
        .map(|(e, _)| e)
        .collect();

    // Phase 3: process each entity
    let mut to_despawn: Vec<Entity> = Vec::new();
    for e in entities {
        // Read position (copy) and check if it's a creature
        let Some(pos) = world.get::<&Position>(e).ok().map(|p| *p) else { continue };
        let is_creature = world.get::<&Creature>(e).is_ok();

        if !is_creature {
            // Generic NPC — just do wander/seek/idle
            if let Ok((_, vel, behavior)) =
                world.query_one_mut::<(&Position, &mut Velocity, &mut Behavior)>(e)
            {
                do_wander_tick(&pos, vel, behavior, map, &mut rng);
            }
            continue;
        }

        // It's a creature — read creature data
        let creature = *world.get::<&Creature>(e).unwrap();
        let behavior_state = world.get::<&Behavior>(e).unwrap().state;
        let speed = world.get::<&Behavior>(e).unwrap().speed;

        // Decide the new state and velocity
        let (new_state, new_vx, new_vy, new_hunger, kill) = match creature.species {
            Species::Prey => {
                let predator_nearby = predator_positions
                    .iter()
                    .any(|&(px, py)| dist(pos.x, pos.y, px, py) < creature.sight_range);

                let (s, vx, vy, h) = ai_prey(
                    &pos, &creature, &behavior_state, speed, predator_nearby,
                    &food_positions, map, &mut rng,
                );
                (s, vx, vy, h, None)
            }
            Species::Predator => {
                ai_predator(
                    &pos, &creature, &behavior_state, speed,
                    &prey_positions, map, &mut rng,
                )
            }
        };

        // Write back
        if let Ok(mut vel) = world.get::<&mut Velocity>(e) {
            vel.dx = new_vx;
            vel.dy = new_vy;
        }
        if let Ok(mut behavior) = world.get::<&mut Behavior>(e) {
            behavior.state = new_state;
        }
        if let Ok(mut c) = world.get::<&mut Creature>(e) {
            c.hunger = new_hunger;
        }
        if let Some(prey_e) = kill {
            to_despawn.push(prey_e);
        }
    }

    // Despawn killed prey
    for e in to_despawn {
        let _ = world.despawn(e);
    }
}

/// Prey AI: eat berries, flee predators, return home.
fn ai_prey(
    pos: &Position,
    creature: &Creature,
    state: &BehaviorState,
    speed: f64,
    predator_nearby: bool,
    food: &[(f64, f64)],
    map: &TileMap,
    rng: &mut impl rand::RngExt,
) -> (BehaviorState, f64, f64, f64) {
    let mut hunger = creature.hunger;
    let pos_copy = *pos;

    match state {
        BehaviorState::AtHome { timer } => {
            if hunger > 0.5 || *timer == 0 {
                (BehaviorState::Wander { timer: 0 }, 0.0, 0.0, hunger)
            } else {
                (BehaviorState::AtHome { timer: timer - 1 }, 0.0, 0.0, hunger)
            }
        }
        BehaviorState::Eating { timer } => {
            hunger = (hunger - 0.01).max(0.0);
            if predator_nearby {
                (BehaviorState::FleeHome, 0.0, 0.0, hunger)
            } else if *timer == 0 || hunger <= 0.0 {
                (BehaviorState::FleeHome, 0.0, 0.0, hunger)
            } else {
                (BehaviorState::Eating { timer: timer - 1 }, 0.0, 0.0, hunger)
            }
        }
        BehaviorState::FleeHome => {
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward(pos, creature.home_x, creature.home_y, speed * 1.5, &mut vel);
            if d < 1.5 {
                (BehaviorState::AtHome { timer: rng.random_range(60..180) }, 0.0, 0.0, hunger)
            } else {
                (BehaviorState::FleeHome, vel.dx, vel.dy, hunger)
            }
        }
        _ => {
            // Wander/Seek/Idle — check for threats and food
            if predator_nearby {
                return (BehaviorState::FleeHome, 0.0, 0.0, hunger);
            }
            if hunger > 0.5 {
                let nearest = food.iter()
                    .map(|&(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((fx, fy, d)) = nearest {
                    if d < 1.5 {
                        return (BehaviorState::Eating { timer: rng.random_range(30..60) }, 0.0, 0.0, hunger);
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward(pos, fx, fy, speed, &mut vel);
                        return (BehaviorState::Seek { target_x: fx, target_y: fy }, vel.dx, vel.dy, hunger);
                    }
                }
            }
            // Default: wander
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let mut bhv = Behavior { state: *state, speed };
            do_wander_tick(&pos_copy, &mut vel, &mut bhv, map, rng);
            (bhv.state, vel.dx, vel.dy, hunger)
        }
    }
}

/// Predator AI: hunt visible prey, wander when not hungry.
/// Returns (new_state, vx, vy, hunger, Option<killed_prey_entity>).
fn ai_predator(
    pos: &Position,
    creature: &Creature,
    state: &BehaviorState,
    speed: f64,
    prey: &[(Entity, f64, f64, bool)],
    map: &TileMap,
    rng: &mut impl rand::RngExt,
) -> (BehaviorState, f64, f64, f64, Option<Entity>) {
    let hunger = creature.hunger;
    let pos_copy = *pos;

    match state {
        BehaviorState::Eating { timer } => {
            let new_hunger = (hunger - 0.01).max(0.0);
            if *timer == 0 || new_hunger <= 0.0 {
                (BehaviorState::Wander { timer: 0 }, 0.0, 0.0, new_hunger, None)
            } else {
                (BehaviorState::Eating { timer: timer - 1 }, 0.0, 0.0, new_hunger, None)
            }
        }
        BehaviorState::Hunting { target_x, target_y } => {
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward(pos, *target_x, *target_y, speed * 1.3, &mut vel);

            if d < 2.0 {
                // Caught prey! Find nearest prey entity to kill
                let killed = prey.iter()
                    .filter(|(_, _, _, at_home)| !at_home)
                    .map(|&(e, px, py, _)| (e, dist(pos.x, pos.y, px, py)))
                    .filter(|(_, d)| *d < 3.5)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                    .map(|(e, _)| e);

                return (
                    BehaviorState::Eating { timer: rng.random_range(40..80) },
                    0.0, 0.0, hunger, killed,
                );
            }
            // Refresh target to nearest visible prey
            let nearest = prey.iter()
                .filter(|(_, _, _, at_home)| !at_home)
                .map(|&(_, px, py, _)| (px, py, dist(pos.x, pos.y, px, py)))
                .filter(|(_, _, d)| *d < creature.sight_range)
                .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
            if let Some((px, py, _)) = nearest {
                (BehaviorState::Hunting { target_x: px, target_y: py }, vel.dx, vel.dy, hunger, None)
            } else {
                (BehaviorState::Wander { timer: 0 }, 0.0, 0.0, hunger, None)
            }
        }
        _ => {
            if hunger > 0.4 {
                let nearest = prey.iter()
                    .filter(|(_, _, _, at_home)| !at_home)
                    .map(|&(_, px, py, _)| (px, py, dist(pos.x, pos.y, px, py)))
                    .filter(|(_, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((px, py, _)) = nearest {
                    let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                    move_toward(pos, px, py, speed * 1.3, &mut vel);
                    return (BehaviorState::Hunting { target_x: px, target_y: py }, vel.dx, vel.dy, hunger, None);
                }
            }
            // Default: wander
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let mut bhv = Behavior { state: *state, speed };
            do_wander_tick(&pos_copy, &mut vel, &mut bhv, map, rng);
            (bhv.state, vel.dx, vel.dy, hunger, None)
        }
    }
}

fn do_wander_tick(
    pos: &Position,
    vel: &mut Velocity,
    behavior: &mut Behavior,
    map: &TileMap,
    rng: &mut impl rand::RngExt,
) {
    match &mut behavior.state {
        BehaviorState::Wander { timer } => {
            if *timer == 0 {
                wander(pos, vel, behavior.speed, map, rng);
                *timer = rng.random_range(20..60);
                if rng.random_range(0..5) == 0 {
                    vel.dx = 0.0;
                    vel.dy = 0.0;
                    behavior.state = BehaviorState::Idle {
                        timer: rng.random_range(30..90),
                    };
                }
            } else {
                *timer -= 1;
            }
        }
        BehaviorState::Seek { target_x, target_y } => {
            let d = move_toward(pos, *target_x, *target_y, behavior.speed, vel);
            if d < 1.5 {
                vel.dx = 0.0;
                vel.dy = 0.0;
                behavior.state = BehaviorState::Idle {
                    timer: rng.random_range(30..90),
                };
            }
        }
        BehaviorState::Idle { timer } => {
            vel.dx = 0.0;
            vel.dy = 0.0;
            if *timer == 0 {
                behavior.state = BehaviorState::Wander { timer: 0 };
            } else {
                *timer -= 1;
            }
        }
        _ => {
            // Other states (Eating, FleeHome, etc.) handled by creature-specific code
            behavior.state = BehaviorState::Wander { timer: 0 };
        }
    }
}

pub fn system_render(world: &World, renderer: &mut dyn Renderer) {
    for (pos, sprite) in world.query::<(&Position, &Sprite)>().iter() {
        let x = pos.x.round() as i32;
        let y = pos.y.round() as i32;
        if x >= 0 && y >= 0 {
            renderer.draw(x as u16, y as u16, sprite.ch, sprite.fg, None);
        }
    }
}

// --- Helpers ---

pub fn spawn_entity(world: &mut World, x: f64, y: f64, dx: f64, dy: f64, ch: char, fg: Color) -> Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx, dy },
        Sprite { ch, fg },
    ))
}

pub fn spawn_npc(world: &mut World, x: f64, y: f64, speed: f64, ch: char, fg: Color) -> Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite { ch, fg },
        Behavior {
            state: BehaviorState::Wander { timer: 0 },
            speed,
        },
    ))
}

pub fn spawn_prey(world: &mut World, x: f64, y: f64, home_x: f64, home_y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite { ch: 'r', fg: Color(180, 140, 80) }, // rabbit-colored
        Behavior {
            state: BehaviorState::AtHome { timer: 30 },
            speed: 0.18,
        },
        Creature {
            species: Species::Prey,
            hunger: 0.2,
            home_x,
            home_y,
            sight_range: 12.0,
        },
    ))
}

pub fn spawn_predator(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite { ch: 'W', fg: Color(160, 50, 50) }, // wolf-colored
        Behavior {
            state: BehaviorState::Wander { timer: 0 },
            speed: 0.22,
        },
        Creature {
            species: Species::Predator,
            hunger: 0.3,
            home_x: x,
            home_y: y,
            sight_range: 18.0,
        },
    ))
}

pub fn spawn_berry_bush(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite { ch: '♦', fg: Color(200, 40, 80) }, // red berries
        FoodSource,
    ))
}

pub fn spawn_den(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite { ch: 'O', fg: Color(140, 100, 60) }, // burrow
        Den,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_renderer::HeadlessRenderer;
    use crate::tilemap::{Terrain, TileMap};

    fn walkable_map(w: usize, h: usize) -> TileMap {
        TileMap::new(w, h, Terrain::Grass)
    }

    #[test]
    fn spawn_and_query() {
        let mut world = World::new();
        spawn_entity(&mut world, 5.0, 3.0, 0.0, 0.0, '@', Color(255, 255, 255));

        let mut count = 0;
        for (pos, sprite) in world.query::<(&Position, &Sprite)>().iter() {
            assert_eq!(pos.x, 5.0);
            assert_eq!(pos.y, 3.0);
            assert_eq!(sprite.ch, '@');
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn movement_system_updates_position() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_entity(&mut world, 10.0, 5.0, 1.5, -0.5, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 11.5);
        assert_eq!(pos.y, 4.5);
    }

    #[test]
    fn movement_accumulates_over_ticks() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_entity(&mut world, 0.0, 0.0, 1.0, 1.0, '@', Color(255, 255, 255));

        for _ in 0..10 {
            system_movement(&mut world, &map);
        }

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 10.0);
    }

    #[test]
    fn collision_blocks_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        // Wall of mountains at x=5
        for y in 0..10 {
            map.set(5, y, Terrain::Mountain);
        }
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        // Should be blocked, position unchanged on x
        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "should be blocked by mountain wall");
    }

    #[test]
    fn collision_bounces_velocity() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::Mountain);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let vel = world.get::<&Velocity>(e).unwrap();
        assert_eq!(vel.dx, -1.0, "velocity should bounce on collision");
    }

    #[test]
    fn slides_along_wall() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        // Wall at x=5
        for y in 0..10 {
            map.set(5, y, Terrain::Mountain);
        }
        // Moving diagonally into wall
        let e = spawn_entity(&mut world, 4.0, 4.0, 1.0, 1.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "x should be blocked");
        assert_eq!(pos.y, 5.0, "y should still move (slide)");
    }

    #[test]
    fn water_blocks_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::Water);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "water should block movement");
    }

    #[test]
    fn render_system_draws_sprites() {
        let mut world = World::new();
        spawn_entity(&mut world, 3.0, 2.0, 0.0, 0.0, '@', Color(0, 255, 0));

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let cell = r.get_cell(3, 2).unwrap();
        assert_eq!(cell.ch, '@');
        assert_eq!(cell.fg, Color(0, 255, 0));
    }

    #[test]
    fn render_skips_negative_positions() {
        let mut world = World::new();
        spawn_entity(&mut world, -5.0, -3.0, 0.0, 0.0, 'X', Color(255, 0, 0));

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        // nothing should have been drawn
        let frame = r.frame_as_string();
        assert!(!frame.contains('X'));
    }

    #[test]
    fn render_skips_out_of_bounds() {
        let mut world = World::new();
        spawn_entity(&mut world, 100.0, 100.0, 0.0, 0.0, 'X', Color(255, 0, 0));

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let frame = r.frame_as_string();
        assert!(!frame.contains('X'));
    }

    #[test]
    fn multiple_entities_render() {
        let mut world = World::new();
        spawn_entity(&mut world, 1.0, 0.0, 0.0, 0.0, 'A', Color(255, 0, 0));
        spawn_entity(&mut world, 3.0, 0.0, 0.0, 0.0, 'B', Color(0, 255, 0));
        spawn_entity(&mut world, 5.0, 0.0, 0.0, 0.0, 'C', Color(0, 0, 255));

        let mut r = HeadlessRenderer::new(10, 3);
        system_render(&world, &mut r);

        assert_eq!(r.get_cell(1, 0).unwrap().ch, 'A');
        assert_eq!(r.get_cell(3, 0).unwrap().ch, 'B');
        assert_eq!(r.get_cell(5, 0).unwrap().ch, 'C');
    }

    #[test]
    fn despawn_removes_entity() {
        let mut world = World::new();
        let e = spawn_entity(&mut world, 5.0, 3.0, 0.0, 0.0, '@', Color(255, 255, 255));
        world.despawn(e).unwrap();

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let frame = r.frame_as_string();
        assert!(!frame.contains('@'));
    }

    #[test]
    fn npc_wanders_and_moves() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let e = spawn_npc(&mut world, 15.0, 15.0, 0.2, '☺', Color(200, 100, 50));

        let start_pos = *world.get::<&Position>(e).unwrap();

        // Run AI + movement for many ticks — NPC will wander eventually
        for _ in 0..500 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
        }

        let end_pos = *world.get::<&Position>(e).unwrap();
        let dist = ((end_pos.x - start_pos.x).powi(2) + (end_pos.y - start_pos.y).powi(2)).sqrt();
        assert!(dist > 0.1, "NPC should have moved from spawn: dist={}", dist);
    }

    #[test]
    fn npc_stays_on_walkable_terrain() {
        let mut world = World::new();
        // Island: grass in center, water everywhere else
        let mut map = TileMap::new(20, 20, Terrain::Water);
        for y in 5..15 {
            for x in 5..15 {
                map.set(x, y, Terrain::Grass);
            }
        }
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.3, '☺', Color(200, 100, 50));

        for _ in 0..500 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
        }

        let pos = *world.get::<&Position>(e).unwrap();
        assert!(map.is_walkable(pos.x, pos.y),
            "NPC should stay on walkable terrain: pos=({}, {})", pos.x, pos.y);
    }

    #[test]
    fn idle_npc_stays_still() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.2, '☺', Color(200, 100, 50));

        // Force into idle state
        {
            let mut behavior = world.get::<&mut Behavior>(e).unwrap();
            behavior.state = BehaviorState::Idle { timer: 100 };
        }

        let start_pos = *world.get::<&Position>(e).unwrap();

        for _ in 0..50 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
        }

        let end_pos = *world.get::<&Position>(e).unwrap();
        assert_eq!(start_pos.x, end_pos.x, "idle NPC should not move");
        assert_eq!(start_pos.y, end_pos.y, "idle NPC should not move");
    }

    #[test]
    fn seek_moves_toward_target() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_npc(&mut world, 5.0, 5.0, 0.3, '☺', Color(200, 100, 50));

        // Force into seek state
        {
            let mut behavior = world.get::<&mut Behavior>(e).unwrap();
            behavior.state = BehaviorState::Seek { target_x: 15.0, target_y: 15.0 };
        }

        // Check position each tick; NPC should get close before transitioning to Idle
        let mut min_dist = f64::INFINITY;
        for _ in 0..200 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
            let pos = *world.get::<&Position>(e).unwrap();
            let dist = ((pos.x - 15.0).powi(2) + (pos.y - 15.0).powi(2)).sqrt();
            if dist < min_dist { min_dist = dist; }
        }

        assert!(min_dist < 2.0, "NPC should reach near target: min_dist={}", min_dist);
    }

    #[test]
    fn hunger_increases_over_time() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        let start_hunger = world.get::<&Creature>(e).unwrap().hunger;

        for _ in 0..100 {
            system_hunger(&mut world);
        }

        let end_hunger = world.get::<&Creature>(e).unwrap().hunger;
        assert!(end_hunger > start_hunger, "hunger should increase: {} -> {}", start_hunger, end_hunger);
        let expected = 0.0005 * 100.0; // prey rate * ticks
        assert!((end_hunger - start_hunger - expected).abs() < 0.001, "hunger should increase by 0.0005/tick for prey");
    }

    #[test]
    fn prey_seeks_food_when_hungry() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        let _bush = spawn_berry_bush(&mut world, 20.0, 10.0);

        // Make prey hungry and wandering
        {
            let mut c = world.get::<&mut Creature>(prey).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        // Run AI — should start seeking food
        system_ai(&mut world, &map);

        let state = world.get::<&Behavior>(prey).unwrap().state;
        match state {
            BehaviorState::Seek { target_x, target_y } => {
                assert!((target_x - 20.0).abs() < 0.1, "should seek food x");
                assert!((target_y - 10.0).abs() < 0.1, "should seek food y");
            }
            _ => panic!("hungry prey should seek food, got: {:?}", state),
        }
    }

    #[test]
    fn prey_flees_when_predator_nearby() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 5.0, 5.0);
        let _predator = spawn_predator(&mut world, 12.0, 10.0); // within sight range

        // Put prey in wander state
        {
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map);

        let state = world.get::<&Behavior>(prey).unwrap().state;
        assert!(matches!(state, BehaviorState::FleeHome),
            "prey should flee when predator nearby, got: {:?}", state);
    }

    #[test]
    fn prey_reaches_home_and_rests() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0); // already at home position

        // Put prey in FleeHome state — should arrive immediately
        {
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::FleeHome;
        }

        system_ai(&mut world, &map);

        let state = world.get::<&Behavior>(prey).unwrap().state;
        assert!(matches!(state, BehaviorState::AtHome { .. }),
            "prey at home position should transition to AtHome, got: {:?}", state);
    }

    #[test]
    fn predator_hunts_visible_prey() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let predator = spawn_predator(&mut world, 10.0, 10.0);
        let _prey = spawn_prey(&mut world, 15.0, 10.0, 25.0, 25.0); // prey in wander state near predator

        // Make predator hungry, prey wandering (not at home)
        {
            let mut c = world.get::<&mut Creature>(predator).unwrap();
            c.hunger = 0.5;
            let mut b = world.get::<&mut Behavior>(_prey).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map);

        let state = world.get::<&Behavior>(predator).unwrap().state;
        assert!(matches!(state, BehaviorState::Hunting { .. }),
            "hungry predator should hunt visible prey, got: {:?}", state);
    }

    #[test]
    fn predator_ignores_prey_at_home() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let predator = spawn_predator(&mut world, 10.0, 10.0);
        let prey = spawn_prey(&mut world, 12.0, 10.0, 12.0, 10.0);

        // Prey is at home (safe), predator is hungry
        {
            let mut c = world.get::<&mut Creature>(predator).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::AtHome { timer: 100 };
        }

        system_ai(&mut world, &map);

        let state = world.get::<&Behavior>(predator).unwrap().state;
        assert!(!matches!(state, BehaviorState::Hunting { .. }),
            "predator should not hunt prey that is at home, got: {:?}", state);
    }

    #[test]
    fn eating_reduces_hunger() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 5.0, 5.0);
        let _bush = spawn_berry_bush(&mut world, 10.0, 10.0);

        // Set prey to eating state with some hunger
        {
            let mut c = world.get::<&mut Creature>(prey).unwrap();
            c.hunger = 0.6;
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::Eating { timer: 30 };
        }

        system_ai(&mut world, &map);

        let hunger = world.get::<&Creature>(prey).unwrap().hunger;
        assert!(hunger < 0.6, "eating should reduce hunger: {}", hunger);
    }
}
