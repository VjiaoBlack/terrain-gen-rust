use hecs::{Entity, World};
use serde::Serialize;

use rand::RngExt;

use crate::renderer::{Color, Renderer};
use crate::simulation::Season;
use crate::tilemap::{TileMap, Terrain};

// --- AI Result ---

/// Result of running the AI system for one tick.
pub struct AiResult {
    pub deposited: Vec<ResourceType>,
    pub food_consumed: u32,
    pub farming_ticks: u32,
    pub mining_ticks: u32,
    pub woodcutting_ticks: u32,
    pub building_ticks: u32,
}

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
    Villager,
}

// Resource types
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ResourceType { Food, Wood, Stone }

/// Marker for stockpile location (where villagers deposit resources).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Stockpile;

/// Resource carried by a villager or stored at stockpile.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CarriedResource {
    pub resource_type: ResourceType,
    pub amount: u32,
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
    /// Prey: captured by a predator, frozen in place until consumed.
    Captured,
    /// Villager: gathering a resource at a location.
    Gathering { timer: u32, resource_type: ResourceType },
    /// Villager: hauling gathered resource back to stockpile.
    Hauling { target_x: f64, target_y: f64, resource_type: ResourceType },
    /// Villager: sleeping at night.
    Sleeping { timer: u32 },
    /// Villager: building at a build site.
    Building { target_x: f64, target_y: f64, timer: u32 },
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum BuildingType {
    Hut,
    Wall,
    Farm,
    Stockpile,
}

impl BuildingType {
    pub fn cost(&self) -> (u32, u32, u32) {
        match self {
            BuildingType::Hut => (0, 5, 2),
            BuildingType::Wall => (0, 1, 1),
            BuildingType::Farm => (2, 3, 0),
            BuildingType::Stockpile => (0, 2, 0),
        }
    }

    pub fn build_time(&self) -> u32 {
        match self {
            BuildingType::Hut => 120,
            BuildingType::Wall => 30,
            BuildingType::Farm => 80,
            BuildingType::Stockpile => 40,
        }
    }

    pub fn size(&self) -> (i32, i32) {
        match self {
            BuildingType::Hut => (3, 3),
            BuildingType::Wall => (1, 1),
            BuildingType::Farm => (3, 3),
            BuildingType::Stockpile => (2, 2),
        }
    }

    pub fn tiles(&self) -> Vec<(i32, i32, Terrain)> {
        match self {
            BuildingType::Hut => {
                let mut tiles = Vec::new();
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingWall));
                    tiles.push((dx, 2, Terrain::BuildingWall));
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor));
                tiles.push((1, 2, Terrain::BuildingFloor)); // door on south side
                tiles
            }
            BuildingType::Wall => vec![(0, 0, Terrain::BuildingWall)],
            BuildingType::Farm => {
                let mut tiles = Vec::new();
                for dy in 0..3 {
                    for dx in 0..3 {
                        tiles.push((dx, dy, Terrain::BuildingFloor));
                    }
                }
                tiles
            }
            BuildingType::Stockpile => {
                let mut tiles = Vec::new();
                for dy in 0..2 {
                    for dx in 0..2 {
                        tiles.push((dx, dy, Terrain::BuildingFloor));
                    }
                }
                tiles
            }
        }
    }

    pub fn all() -> &'static [BuildingType] {
        &[BuildingType::Hut, BuildingType::Wall, BuildingType::Farm, BuildingType::Stockpile]
    }

    pub fn name(&self) -> &'static str {
        match self {
            BuildingType::Hut => "Hut",
            BuildingType::Wall => "Wall",
            BuildingType::Farm => "Farm",
            BuildingType::Stockpile => "Stockpile",
        }
    }
}

/// A build site entity — placed by the player, worked on by villagers.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct BuildSite {
    pub building_type: BuildingType,
    pub progress: u32,
    pub required: u32,
    pub assigned: bool,
}

/// Marker for a completed farm plot — grows crops and produces food.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FarmPlot {
    pub growth: f64,        // 0.0 to 1.0
    pub harvest_ready: bool,
}

/// Marker component for berry bushes (food source for prey).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FoodSource;

/// Marker component for stone deposits (mineable by villagers).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct StoneDeposit;

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
pub fn system_hunger(world: &mut World, hunger_mult: f64) {
    for creature in world.query_mut::<&mut Creature>() {
        let rate = match creature.species {
            Species::Prey => 0.0005,
            Species::Predator => 0.0006, // predators burn slightly more
            Species::Villager => 0.00015, // villagers burn slowly — settlements need time to establish
        };
        creature.hunger = (creature.hunger + rate * hunger_mult).min(1.0);
    }
}

/// Despawn any creature that has starved (hunger >= 1.0).
pub fn system_death(world: &mut World) -> Vec<Entity> {
    let starved: Vec<Entity> = world
        .query::<(Entity, &Creature)>()
        .iter()
        .filter(|(_, c)| c.hunger >= 1.0)
        .map(|(e, _)| e)
        .collect();
    for &e in &starved {
        let _ = world.despawn(e);
    }
    starved
}

/// AI system: updates velocity based on behavior, species, and world state.
pub fn system_ai(world: &mut World, map: &TileMap, wolf_aggression: f64, stockpile_food: u32) -> AiResult {
    let mut rng = rand::rng();
    let mut deposited_resources: Vec<ResourceType> = Vec::new();
    let mut food_consumed: u32 = 0;
    let mut farming_ticks: u32 = 0;
    let mut mining_ticks: u32 = 0;
    let mut woodcutting_ticks: u32 = 0;
    let mut building_ticks: u32 = 0;

    // Phase 1: snapshot world state (positions of food, prey, predators, stockpiles)
    let food_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &FoodSource)>()
        .iter()
        .map(|(pos, _)| (pos.x, pos.y))
        .collect();

    let prey_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
        .filter(|(_, _, c, _)| c.species == Species::Prey)
        .map(|(e, p, _, b)| (e, p.x, p.y, matches!(b.state, BehaviorState::AtHome { .. } | BehaviorState::Captured)))
        .collect();

    let villager_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
        .filter(|(_, _, c, _)| c.species == Species::Villager)
        .map(|(e, p, _, b)| (e, p.x, p.y, matches!(b.state, BehaviorState::Captured)))
        .collect();

    let predator_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &Creature)>()
        .iter()
        .filter(|(_, c)| c.species == Species::Predator)
        .map(|(p, _)| (p.x, p.y))
        .collect();

    let stockpile_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &Stockpile)>()
        .iter()
        .map(|(pos, _)| (pos.x, pos.y))
        .collect();

    let build_site_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &BuildSite)>()
        .iter()
        .map(|(e, pos, site)| (e, pos.x, pos.y, site.assigned))
        .collect();

    let stone_deposit_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &StoneDeposit)>()
        .iter()
        .map(|(pos, _)| (pos.x, pos.y))
        .collect();

    // Phase 2: collect entity IDs with Behavior
    let entities: Vec<Entity> = world
        .query::<(Entity, &Behavior)>()
        .iter()
        .map(|(e, _)| e)
        .collect();

    // Phase 3: process each entity
    let mut to_capture: Vec<Entity> = Vec::new();
    let mut to_despawn: Vec<Entity> = Vec::new();
    let mut build_progress: Vec<(f64, f64)> = Vec::new(); // positions where building work happened
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

        // Captured prey: frozen, no AI — wait for predator to finish eating
        if matches!(behavior_state, BehaviorState::Captured) {
            continue;
        }

        // Decide the new state and velocity
        let (new_state, new_vx, new_vy, new_hunger, kill, deposited) = match creature.species {
            Species::Prey => {
                let predator_nearby = predator_positions
                    .iter()
                    .any(|&(px, py)| dist(pos.x, pos.y, px, py) < creature.sight_range);

                let (s, vx, vy, h) = ai_prey(
                    &pos, &creature, &behavior_state, speed, predator_nearby,
                    &food_positions, map, &mut rng,
                );
                (s, vx, vy, h, None, None)
            }
            Species::Predator => {
                let (s, vx, vy, h, k) = ai_predator(
                    &pos, &creature, &behavior_state, speed,
                    &prey_positions, &villager_positions, wolf_aggression,
                    map, &mut rng,
                );
                (s, vx, vy, h, k, None)
            }
            Species::Villager => {
                let predator_nearby = predator_positions
                    .iter()
                    .any(|&(px, py)| dist(pos.x, pos.y, px, py) < creature.sight_range);

                let has_food = stockpile_food.saturating_sub(food_consumed) > 0;
                let was_eating = matches!(behavior_state, BehaviorState::Eating { .. });
                let near_food_source = food_positions.iter()
                    .any(|&(fx, fy)| dist(pos.x, pos.y, fx, fy) < 2.0);

                let (s, vx, vy, h, dep, claim_site) = ai_villager(
                    &pos, &creature, &behavior_state, speed, predator_nearby,
                    &food_positions, &stockpile_positions, &build_site_positions,
                    &stone_deposit_positions, has_food, map, &mut rng,
                );

                // If villager just started eating near stockpile (not near berry bush), consume food
                if matches!(s, BehaviorState::Eating { .. }) && !was_eating && !near_food_source {
                    food_consumed += 1;
                }
                // If villager claims a build site, mark it assigned
                if let Some(site_entity) = claim_site {
                    if let Ok(mut site) = world.get::<&mut BuildSite>(site_entity) {
                        site.assigned = true;
                    }
                }
                (s, vx, vy, h, None, dep)
            }
        };

        if let Some(resource) = deposited {
            deposited_resources.push(resource);
        }

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
        // Track build progress and activity for skills
        if creature.species == Species::Villager {
            match new_state {
                BehaviorState::Building { target_x, target_y, .. } => {
                    build_progress.push((target_x, target_y));
                    building_ticks += 1;
                }
                BehaviorState::Gathering { resource_type: ResourceType::Wood, .. } => {
                    woodcutting_ticks += 1;
                }
                BehaviorState::Gathering { resource_type: ResourceType::Stone, .. } => {
                    mining_ticks += 1;
                }
                BehaviorState::Gathering { resource_type: ResourceType::Food, .. } => {
                    farming_ticks += 1;
                }
                _ => {}
            }
        } else if let BehaviorState::Building { target_x, target_y, .. } = new_state {
            build_progress.push((target_x, target_y));
        }
        if let Some(prey_e) = kill {
            // If wolf just caught prey (entering Eating), mark prey as Captured
            // If wolf finished eating (leaving Eating), despawn the prey
            if matches!(new_state, BehaviorState::Eating { .. }) {
                // Capture: freeze the prey in place
                to_capture.push(prey_e);
            } else {
                // Done eating: remove the carcass
                to_despawn.push(prey_e);
            }
        }
    }

    // Mark captured prey
    for e in to_capture {
        if let Ok(mut behavior) = world.get::<&mut Behavior>(e) {
            behavior.state = BehaviorState::Captured;
        }
        if let Ok(mut vel) = world.get::<&mut Velocity>(e) {
            vel.dx = 0.0;
            vel.dy = 0.0;
        }
    }

    // Despawn consumed prey
    for e in to_despawn {
        let _ = world.despawn(e);
    }

    // Increment progress on build sites where villagers are working
    for (bx, by) in build_progress {
        for (pos, site) in world.query_mut::<(&Position, &mut BuildSite)>() {
            if (pos.x - bx).abs() < 1.5 && (pos.y - by).abs() < 1.5 {
                site.progress += 1;
                break;
            }
        }
    }

    AiResult {
        deposited: deposited_resources,
        food_consumed,
        farming_ticks,
        mining_ticks,
        woodcutting_ticks,
        building_ticks,
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
    villagers: &[(Entity, f64, f64, bool)],
    wolf_aggression: f64,
    map: &TileMap,
    rng: &mut impl rand::RngExt,
) -> (BehaviorState, f64, f64, f64, Option<Entity>) {
    let hunger = creature.hunger;
    let pos_copy = *pos;

    // Build combined target list: always include prey, add villagers when desperate
    let targets: Vec<(Entity, f64, f64, bool)> = if hunger > wolf_aggression {
        prey.iter().chain(villagers.iter()).copied().collect()
    } else {
        prey.to_vec()
    };

    match state {
        BehaviorState::Eating { timer } => {
            let new_hunger = (hunger - 0.01).max(0.0);
            if *timer == 0 || new_hunger <= 0.0 {
                // Done eating — signal to despawn the captured prey/villager nearby
                let victim = targets.iter()
                    .map(|&(e, px, py, _)| (e, dist(pos.x, pos.y, px, py)))
                    .filter(|(_, d)| *d < 3.0)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                    .map(|(e, _)| e);
                (BehaviorState::Wander { timer: 0 }, 0.0, 0.0, new_hunger, victim)
            } else {
                (BehaviorState::Eating { timer: timer - 1 }, 0.0, 0.0, new_hunger, None)
            }
        }
        BehaviorState::Hunting { .. } => {
            // Find nearest visible target to chase (refreshes target each tick)
            let nearest = targets.iter()
                .filter(|(_, _, _, at_home)| !at_home)
                .map(|&(e, px, py, _)| (e, px, py, dist(pos.x, pos.y, px, py)))
                .filter(|(_, _, _, d)| *d < creature.sight_range)
                .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap());

            if let Some((target_e, px, py, d)) = nearest {
                if d < 2.0 {
                    // Caught target! Mark it as captured, start eating
                    return (
                        BehaviorState::Eating { timer: rng.random_range(40..80) },
                        0.0, 0.0, hunger, Some(target_e),
                    );
                }
                // Keep chasing
                let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                move_toward(pos, px, py, speed * 1.3, &mut vel);
                (BehaviorState::Hunting { target_x: px, target_y: py }, vel.dx, vel.dy, hunger, None)
            } else {
                // Lost sight of all targets — give up
                (BehaviorState::Wander { timer: 0 }, 0.0, 0.0, hunger, None)
            }
        }
        _ => {
            if hunger > 0.4 {
                let nearest = targets.iter()
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
        BehaviorState::Captured => {
            // Frozen — do nothing
            vel.dx = 0.0;
            vel.dy = 0.0;
        }
        BehaviorState::Gathering { .. } | BehaviorState::Hauling { .. } | BehaviorState::Sleeping { .. } | BehaviorState::Building { .. } => {
            // Handled by creature-specific code
            vel.dx = 0.0;
            vel.dy = 0.0;
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

pub fn spawn_stone_deposit(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite { ch: '●', fg: Color(150, 140, 130) }, // grey stone
        StoneDeposit,
    ))
}

pub fn spawn_villager(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite { ch: 'V', fg: Color(100, 200, 255) }, // villager: light blue
        Behavior {
            state: BehaviorState::Idle { timer: 10 },
            speed: 0.15,
        },
        Creature {
            species: Species::Villager,
            hunger: 0.1,
            home_x: x,
            home_y: y,
            sight_range: 15.0,
        },
    ))
}

pub fn spawn_build_site(world: &mut World, x: f64, y: f64, building_type: BuildingType) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite { ch: '#', fg: Color(200, 180, 100) },
        BuildSite {
            building_type,
            progress: 0,
            required: building_type.build_time(),
            assigned: false,
        },
    ))
}

pub fn spawn_stockpile(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite { ch: '■', fg: Color(180, 140, 60) }, // wooden stockpile
        Stockpile,
    ))
}

pub fn spawn_farm_plot(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite { ch: '·', fg: Color(120, 80, 30) }, // starts as dirt
        FarmPlot { growth: 0.0, harvest_ready: false },
    ))
}

/// Grow farm plots based on season and auto-harvest when ready.
/// Returns the amount of food produced this tick.
pub fn system_farms(world: &mut World, season: Season) -> u32 {
    let growth_rate = match season {
        Season::Spring => 0.002,
        Season::Summer => 0.003,
        Season::Autumn => 0.001,
        Season::Winter => 0.0,
    };

    // Pass 1: advance growth
    let mut food_produced = 0u32;
    for farm in world.query_mut::<&mut FarmPlot>() {
        if farm.harvest_ready {
            // Auto-harvest
            farm.growth = 0.0;
            farm.harvest_ready = false;
            food_produced += 3;
        } else {
            farm.growth += growth_rate;
            if farm.growth >= 1.0 {
                farm.growth = 1.0;
                farm.harvest_ready = true;
            }
        }
    }

    // Pass 2: update sprite visuals based on growth stage
    for (farm, sprite) in world.query_mut::<(&FarmPlot, &mut Sprite)>() {
        if farm.harvest_ready {
            sprite.fg = Color(220, 200, 40); // harvest ready — gold
            sprite.ch = '♣';
        } else if farm.growth < 0.3 {
            sprite.fg = Color(120, 80, 30);  // dirt
            sprite.ch = '·';
        } else if farm.growth < 0.7 {
            sprite.fg = Color(80, 160, 40);  // growing
            sprite.ch = '♠';
        } else {
            sprite.fg = Color(60, 180, 40);  // mature
            sprite.ch = '"';
        }
    }

    food_produced
}

/// Find the nearest tile of a given terrain type within a radius.
/// For walkable terrain (e.g. Forest), returns the tile position directly.
/// For non-walkable terrain (e.g. Mountain), returns an adjacent walkable tile.
fn find_nearest_terrain(pos: &Position, map: &TileMap, terrain: Terrain, radius: f64) -> Option<(f64, f64)> {
    let cx = pos.x.round() as i32;
    let cy = pos.y.round() as i32;
    let r = radius as i32;
    let mut best: Option<(f64, f64, f64)> = None;

    let terrain_walkable = terrain.is_walkable();

    for dy in -r..=r {
        for dx in -r..=r {
            let tx = cx + dx;
            let ty = cy + dy;
            if tx >= 0 && ty >= 0 {
                if let Some(t) = map.get(tx as usize, ty as usize) {
                    if *t == terrain {
                        if terrain_walkable {
                            // Walkable terrain (e.g. Forest): stand on the tile directly
                            let d = dist(pos.x, pos.y, tx as f64, ty as f64);
                            if best.is_none() || d < best.unwrap().2 {
                                best = Some((tx as f64, ty as f64, d));
                            }
                        } else {
                            // Non-walkable terrain (e.g. Mountain): find adjacent walkable tile
                            for &(ax, ay) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                                let wx = tx + ax;
                                let wy = ty + ay;
                                if map.is_walkable(wx as f64, wy as f64) {
                                    let d = dist(pos.x, pos.y, wx as f64, wy as f64);
                                    if best.is_none() || d < best.unwrap().2 {
                                        best = Some((wx as f64, wy as f64, d));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    best.map(|(x, y, _)| (x, y))
}

/// Villager AI: gather resources, eat when hungry, flee predators, build.
/// Returns (new_state, vx, vy, hunger, Option<ResourceType> deposited, Option<Entity> claimed_build_site).
fn ai_villager(
    pos: &Position,
    creature: &Creature,
    state: &BehaviorState,
    speed: f64,
    predator_nearby: bool,
    food: &[(f64, f64)],
    stockpile: &[(f64, f64)],
    build_sites: &[(Entity, f64, f64, bool)],
    stone_deposits: &[(f64, f64)],
    has_stockpile_food: bool,
    map: &TileMap,
    rng: &mut impl rand::RngExt,
) -> (BehaviorState, f64, f64, f64, Option<ResourceType>, Option<Entity>) {
    let mut hunger = creature.hunger;
    let pos_copy = *pos;

    match state {
        BehaviorState::Eating { timer } => {
            hunger = (hunger - 0.01).max(0.0);
            if predator_nearby {
                (BehaviorState::FleeHome, 0.0, 0.0, hunger, None, None)
            } else if *timer == 0 || hunger <= 0.0 {
                (BehaviorState::Idle { timer: rng.random_range(20..60) }, 0.0, 0.0, hunger, None, None)
            } else {
                (BehaviorState::Eating { timer: timer - 1 }, 0.0, 0.0, hunger, None, None)
            }
        }
        BehaviorState::FleeHome => {
            // Flee toward nearest stockpile (or home)
            let (hx, hy) = stockpile.iter()
                .map(|&(sx, sy)| (sx, sy, dist(pos.x, pos.y, sx, sy)))
                .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
                .map(|(sx, sy, _)| (sx, sy))
                .unwrap_or((creature.home_x, creature.home_y));

            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward(pos, hx, hy, speed * 1.5, &mut vel);
            if d < 1.5 {
                (BehaviorState::Idle { timer: rng.random_range(30..90) }, 0.0, 0.0, hunger, None, None)
            } else {
                (BehaviorState::FleeHome, vel.dx, vel.dy, hunger, None, None)
            }
        }
        BehaviorState::Gathering { timer, resource_type } => {
            if predator_nearby {
                return (BehaviorState::FleeHome, 0.0, 0.0, hunger, None, None);
            }
            if *timer == 0 {
                // Done gathering — haul to nearest stockpile
                let (hx, hy) = stockpile.iter()
                    .map(|&(sx, sy)| (sx, sy, dist(pos.x, pos.y, sx, sy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
                    .map(|(sx, sy, _)| (sx, sy))
                    .unwrap_or((creature.home_x, creature.home_y));
                let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                move_toward(pos, hx, hy, speed, &mut vel);
                (BehaviorState::Hauling { target_x: hx, target_y: hy, resource_type: *resource_type }, vel.dx, vel.dy, hunger, None, None)
            } else {
                (BehaviorState::Gathering { timer: timer - 1, resource_type: *resource_type }, 0.0, 0.0, hunger, None, None)
            }
        }
        BehaviorState::Hauling { target_x, target_y, resource_type } => {
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward(pos, *target_x, *target_y, speed, &mut vel);
            if d < 1.5 {
                // Deposited resource at stockpile
                (BehaviorState::Idle { timer: rng.random_range(20..60) }, 0.0, 0.0, hunger, Some(*resource_type), None)
            } else {
                (BehaviorState::Hauling { target_x: *target_x, target_y: *target_y, resource_type: *resource_type }, vel.dx, vel.dy, hunger, None, None)
            }
        }
        BehaviorState::Sleeping { timer } => {
            if *timer == 0 {
                (BehaviorState::Idle { timer: 10 }, 0.0, 0.0, hunger, None, None)
            } else {
                (BehaviorState::Sleeping { timer: timer - 1 }, 0.0, 0.0, hunger, None, None)
            }
        }
        BehaviorState::Building { target_x, target_y, timer } => {
            if predator_nearby {
                return (BehaviorState::FleeHome, 0.0, 0.0, hunger, None, None);
            }
            if *timer == 0 {
                // Done building this round
                (BehaviorState::Idle { timer: rng.random_range(20..60) }, 0.0, 0.0, hunger, None, None)
            } else {
                (BehaviorState::Building { target_x: *target_x, target_y: *target_y, timer: timer - 1 }, 0.0, 0.0, hunger, None, None)
            }
        }
        _ => {
            // Wander/Seek/Idle — check for threats, food, and gathering
            if predator_nearby {
                return (BehaviorState::FleeHome, 0.0, 0.0, hunger, None, None);
            }

            // Eat: if hungry and near food (eat early to avoid starvation)
            if hunger > 0.4 {
                let nearest_food = food.iter()
                    .map(|&(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((fx, fy, d)) = nearest_food {
                    if d < 1.5 {
                        return (BehaviorState::Eating { timer: rng.random_range(30..60) }, 0.0, 0.0, hunger, None, None);
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward(pos, fx, fy, speed, &mut vel);
                        return (BehaviorState::Seek { target_x: fx, target_y: fy }, vel.dx, vel.dy, hunger, None, None);
                    }
                }
                // No berry bush reachable — eat from stockpile if food available
                if has_stockpile_food {
                    let nearest_stockpile = stockpile.iter()
                        .map(|&(sx, sy)| (sx, sy, dist(pos.x, pos.y, sx, sy)))
                        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                    if let Some((sx, sy, d)) = nearest_stockpile {
                        if d < 1.5 {
                            return (BehaviorState::Eating { timer: rng.random_range(20..40) }, 0.0, 0.0, hunger, None, None);
                        } else {
                            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                            move_toward(pos, sx, sy, speed, &mut vel);
                            return (BehaviorState::Seek { target_x: sx, target_y: sy }, vel.dx, vel.dy, hunger, None, None);
                        }
                    }
                }
            }

            // Build: if not too hungry and there are unassigned build sites
            if hunger < 0.4 {
                let nearest_site = build_sites.iter()
                    .filter(|(_, _, _, assigned)| !assigned)
                    .map(|&(e, bx, by, _)| (e, bx, by, dist(pos.x, pos.y, bx, by)))
                    .filter(|(_, _, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap());
                if let Some((site_e, bx, by, d)) = nearest_site {
                    if d < 1.5 {
                        return (BehaviorState::Building { target_x: bx, target_y: by, timer: 30 }, 0.0, 0.0, hunger, None, Some(site_e));
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward(pos, bx, by, speed, &mut vel);
                        return (BehaviorState::Seek { target_x: bx, target_y: by }, vel.dx, vel.dy, hunger, None, Some(site_e));
                    }
                }
            }

            // Gather wood: if not too hungry, find nearest Forest tile
            if hunger < 0.4 {
                if let Some((fx, fy)) = find_nearest_terrain(pos, map, Terrain::Forest, creature.sight_range) {
                    let d = dist(pos.x, pos.y, fx, fy);
                    if d < 1.5 {
                        return (BehaviorState::Gathering { timer: 60, resource_type: ResourceType::Wood }, 0.0, 0.0, hunger, None, None);
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward(pos, fx, fy, speed, &mut vel);
                        return (BehaviorState::Seek { target_x: fx, target_y: fy }, vel.dx, vel.dy, hunger, None, None);
                    }
                }
                // Gather stone: prefer nearby StoneDeposit entities, fall back to Mountain-adjacent tiles
                let nearest_deposit = stone_deposits.iter()
                    .map(|&(dx, dy)| (dx, dy, dist(pos.x, pos.y, dx, dy)))
                    .filter(|(_, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                let stone_target = nearest_deposit.map(|(dx, dy, d)| (dx, dy, d))
                    .or_else(|| find_nearest_terrain(pos, map, Terrain::Mountain, creature.sight_range)
                        .map(|(mx, my)| (mx, my, dist(pos.x, pos.y, mx, my))));
                if let Some((sx, sy, d)) = stone_target {
                    if d < 1.5 {
                        return (BehaviorState::Gathering { timer: 60, resource_type: ResourceType::Stone }, 0.0, 0.0, hunger, None, None);
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward(pos, sx, sy, speed, &mut vel);
                        return (BehaviorState::Seek { target_x: sx, target_y: sy }, vel.dx, vel.dy, hunger, None, None);
                    }
                }
            }

            // Default: wander
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let mut bhv = Behavior { state: *state, speed };
            do_wander_tick(&pos_copy, &mut vel, &mut bhv, map, rng);
            (bhv.state, vel.dx, vel.dy, hunger, None, None)
        }
    }
}

/// Breeding system: prey breed at dens in spring/summer, wolves breed when well-fed.
pub fn system_breeding(world: &mut World, season: Season) {
    let mut rng = rand::rng();

    // Only breed in Spring and Summer
    if !matches!(season, Season::Spring | Season::Summer) {
        return;
    }

    // Count prey per den
    let mut den_prey_count: std::collections::HashMap<(i32, i32), u32> =
        std::collections::HashMap::new();
    for creature in world.query::<&Creature>().iter() {
        if creature.species == Species::Prey {
            let key = (creature.home_x.round() as i32, creature.home_y.round() as i32);
            *den_prey_count.entry(key).or_insert(0) += 1;
        }
    }

    // Find prey breeding candidates: at home with low hunger, den not full
    let candidates: Vec<(f64, f64)> = world
        .query::<(&Creature, &Behavior)>()
        .iter()
        .filter(|(c, b)| {
            c.species == Species::Prey
                && c.hunger < 0.3
                && matches!(b.state, BehaviorState::AtHome { .. })
        })
        .filter_map(|(c, _)| {
            let key = (c.home_x.round() as i32, c.home_y.round() as i32);
            let count = den_prey_count.get(&key).copied().unwrap_or(0);
            if count < 3 {
                Some((c.home_x, c.home_y))
            } else {
                None
            }
        })
        .collect();

    // Spawn prey with probability
    let mut prey_to_spawn: Vec<(f64, f64)> = Vec::new();
    for (hx, hy) in candidates {
        if rng.random_range(0u32..500) == 0 {
            prey_to_spawn.push((hx, hy));
        }
    }

    for (hx, hy) in prey_to_spawn {
        let ox = rng.random_range(-2i32..3) as f64;
        let oy = rng.random_range(-2i32..3) as f64;
        spawn_prey(world, hx + ox, hy + oy, hx, hy);
    }

    // Wolf breeding
    let wolf_count = world
        .query::<&Creature>()
        .iter()
        .filter(|c| c.species == Species::Predator)
        .count();

    if wolf_count < 6 {
        let wolf_candidates: Vec<(f64, f64)> = world
            .query::<(&Position, &Creature, &Behavior)>()
            .iter()
            .filter(|(_, c, b)| {
                c.species == Species::Predator
                    && c.hunger < 0.2
                    && matches!(
                        b.state,
                        BehaviorState::Wander { .. } | BehaviorState::Idle { .. }
                    )
            })
            .map(|(p, _, _)| (p.x, p.y))
            .collect();

        for (px, py) in wolf_candidates {
            if rng.random_range(0u32..1000) == 0 {
                spawn_predator(
                    world,
                    px + rng.random_range(-3i32..4) as f64,
                    py + rng.random_range(-3i32..4) as f64,
                );
            }
        }
    }
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
            system_ai(&mut world, &map, 0.4, 0);
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
            system_ai(&mut world, &map, 0.4, 0);
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
            system_ai(&mut world, &map, 0.4, 0);
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
            system_ai(&mut world, &map, 0.4, 0);
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
            system_hunger(&mut world, 1.0);
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
        system_ai(&mut world, &map, 0.4, 0);

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

        system_ai(&mut world, &map, 0.4, 0);

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

        system_ai(&mut world, &map, 0.4, 0);

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

        system_ai(&mut world, &map, 0.4, 0);

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

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(predator).unwrap().state;
        assert!(!matches!(state, BehaviorState::Hunting { .. }),
            "predator should not hunt prey that is at home, got: {:?}", state);
    }

    #[test]
    fn wolf_hunts_and_kills_rabbit() {
        let mut world = World::new();
        let map = walkable_map(50, 50);
        // Wolf at (10,10), prey at (15,10) — prey's home far away at (40,40)
        let wolf = spawn_predator(&mut world, 10.0, 10.0);
        let rabbit = spawn_prey(&mut world, 15.0, 10.0, 40.0, 40.0);

        // Make wolf hungry, rabbit wandering
        {
            let mut c = world.get::<&mut Creature>(wolf).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(rabbit).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let mut wolf_ate = false;
        let mut rabbit_alive = true;

        for tick in 0..300 {
            system_ai(&mut world, &map, 0.4, 0);
            system_movement(&mut world, &map);

            let wolf_state = world.get::<&Behavior>(wolf).unwrap().state;
            let rabbit_exists = world.get::<&Position>(rabbit).is_ok();

            if matches!(wolf_state, BehaviorState::Eating { .. }) {
                wolf_ate = true;
            }
            if !rabbit_exists {
                rabbit_alive = false;
                eprintln!("tick {}: rabbit despawned, wolf state: {:?}", tick, wolf_state);
                break;
            }

            if tick < 5 || tick % 20 == 0 {
                let wp = *world.get::<&Position>(wolf).unwrap();
                let rp = *world.get::<&Position>(rabbit).unwrap();
                let rs = world.get::<&Behavior>(rabbit).unwrap().state;
                let d = ((wp.x - rp.x).powi(2) + (wp.y - rp.y).powi(2)).sqrt();
                eprintln!("tick {}: wolf({:.1},{:.1}) {:?}  rabbit({:.1},{:.1}) {:?}  dist={:.1}",
                    tick, wp.x, wp.y, wolf_state, rp.x, rp.y, rs, d);
            }
        }

        assert!(wolf_ate, "wolf should have entered Eating state");
        assert!(!rabbit_alive, "rabbit should have been killed");
    }

    #[test]
    fn full_ecosystem_simulation() {
        // Simulate the actual game ecosystem setup for 1000 ticks
        let mut world = World::new();
        let map = walkable_map(60, 60);

        // Den at (10,10), prey starts near den, bush at (30,30), wolf at (25,25)
        spawn_den(&mut world, 10.0, 10.0);
        let rabbit = spawn_prey(&mut world, 11.0, 11.0, 10.0, 10.0);
        spawn_berry_bush(&mut world, 30.0, 30.0);
        let wolf = spawn_predator(&mut world, 25.0, 25.0);

        // Make wolf hungry
        {
            let mut c = world.get::<&mut Creature>(wolf).unwrap();
            c.hunger = 0.6;
        }

        let mut states_seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for tick in 0..1000 {
            system_hunger(&mut world, 1.0);
            system_ai(&mut world, &map, 0.4, 0);
            system_movement(&mut world, &map);

            let ws = world.get::<&Behavior>(wolf).unwrap().state;
            let state_name = format!("{:?}", ws).split('{').next().unwrap_or("?").split('(').next().unwrap_or("?").trim().to_string();
            states_seen.insert(format!("wolf:{}", state_name));

            if let Ok(rb) = world.get::<&Behavior>(rabbit) {
                let rstate = format!("{:?}", rb.state).split('{').next().unwrap_or("?").split('(').next().unwrap_or("?").trim().to_string();
                states_seen.insert(format!("rabbit:{}", rstate));
            }

            if tick % 100 == 0 {
                let wh = world.get::<&Creature>(wolf).unwrap().hunger;
                let rabbit_alive = world.get::<&Position>(rabbit).is_ok();
                eprintln!("tick {}: wolf hunger={:.2} state={:?} rabbit_alive={}",
                    tick, wh, ws, rabbit_alive);
            }
        }

        eprintln!("all states seen: {:?}", states_seen);
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

        system_ai(&mut world, &map, 0.4, 0);

        let hunger = world.get::<&Creature>(prey).unwrap().hunger;
        assert!(hunger < 0.6, "eating should reduce hunger: {}", hunger);
    }

    #[test]
    fn villager_seeks_food_when_hungry() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        let _bush = spawn_berry_bush(&mut world, 20.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        // Make villager hungry and wandering
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        match state {
            BehaviorState::Seek { target_x, target_y } => {
                assert!((target_x - 20.0).abs() < 0.1, "should seek food x");
                assert!((target_y - 10.0).abs() < 0.1, "should seek food y");
            }
            _ => panic!("hungry villager should seek food, got: {:?}", state),
        }
    }

    #[test]
    fn villager_flees_predator() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        let _predator = spawn_predator(&mut world, 12.0, 10.0); // within sight range
        spawn_stockpile(&mut world, 5.0, 5.0);

        // Put villager in wander state
        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(matches!(state, BehaviorState::FleeHome),
            "villager should flee when predator nearby, got: {:?}", state);
    }

    #[test]
    fn villager_gathers_wood() {
        let mut world = World::new();
        // Map with forest tiles nearby
        let mut map = walkable_map(30, 30);
        map.set(12, 10, Terrain::Forest);
        map.set(13, 10, Terrain::Forest);

        let villager = spawn_villager(&mut world, 12.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        // Low hunger, wandering — should gather wood
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(matches!(state, BehaviorState::Gathering { resource_type: ResourceType::Wood, .. }),
            "villager near forest with low hunger should gather wood, got: {:?}", state);
    }

    #[test]
    fn villager_hauls_to_stockpile() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        // Set villager to Gathering with timer about to expire
        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Gathering { timer: 0, resource_type: ResourceType::Wood };
        }

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        match state {
            BehaviorState::Hauling { target_x, target_y, resource_type } => {
                assert!((target_x - 5.0).abs() < 0.1, "should haul toward stockpile x");
                assert!((target_y - 5.0).abs() < 0.1, "should haul toward stockpile y");
                assert_eq!(resource_type, ResourceType::Wood, "should haul wood");
            }
            _ => panic!("villager after gathering should haul to stockpile, got: {:?}", state),
        }
    }

    #[test]
    fn villager_deposits_resource() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 5.0, 5.0); // at stockpile position
        spawn_stockpile(&mut world, 5.0, 5.0);

        // Set villager to hauling toward stockpile (already there)
        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Hauling { target_x: 5.0, target_y: 5.0, resource_type: ResourceType::Wood };
        }

        let result = system_ai(&mut world, &map, 0.4, 0);

        assert_eq!(result.deposited.len(), 1, "should deposit one resource");
        assert_eq!(result.deposited[0], ResourceType::Wood, "should deposit wood");

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(matches!(state, BehaviorState::Idle { .. }),
            "villager should be idle after depositing, got: {:?}", state);
    }

    #[test]
    fn building_wall_blocks_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::BuildingWall);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "BuildingWall should block movement");
    }

    #[test]
    fn building_floor_is_walkable() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::BuildingFloor);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 5.0, "BuildingFloor should be walkable");
    }

    #[test]
    fn villager_builds_at_site() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);
        let _site = spawn_build_site(&mut world, 10.0, 10.0, BuildingType::Wall);

        // Low hunger, idle — should find build site and start building
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(matches!(state, BehaviorState::Building { .. }),
            "villager near build site with low hunger should start building, got: {:?}", state);
    }

    #[test]
    fn build_site_completes() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);
        let site = spawn_build_site(&mut world, 10.0, 10.0, BuildingType::Wall);

        // Set build site progress to almost complete
        {
            let mut s = world.get::<&mut BuildSite>(site).unwrap();
            s.progress = s.required - 1;
        }

        // Put villager in building state at the site
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Building { target_x: 10.0, target_y: 10.0, timer: 5 };
        }

        system_ai(&mut world, &map, 0.4, 0);

        // Build site should now be complete (progress >= required)
        let s = world.get::<&BuildSite>(site).unwrap();
        assert!(s.progress >= s.required,
            "build site should be complete: progress={} required={}", s.progress, s.required);
    }

    #[test]
    fn building_type_costs_and_sizes() {
        assert_eq!(BuildingType::Hut.cost(), (0, 5, 2));
        assert_eq!(BuildingType::Wall.cost(), (0, 1, 1));
        assert_eq!(BuildingType::Farm.cost(), (2, 3, 0));
        assert_eq!(BuildingType::Stockpile.cost(), (0, 2, 0));

        assert_eq!(BuildingType::Hut.size(), (3, 3));
        assert_eq!(BuildingType::Wall.size(), (1, 1));
        assert_eq!(BuildingType::Farm.size(), (3, 3));
        assert_eq!(BuildingType::Stockpile.size(), (2, 2));

        assert_eq!(BuildingType::Hut.build_time(), 120);
        assert_eq!(BuildingType::Wall.build_time(), 30);
        assert_eq!(BuildingType::Farm.build_time(), 80);
        assert_eq!(BuildingType::Stockpile.build_time(), 40);

        // Wall tiles should contain exactly one BuildingWall
        let wall_tiles = BuildingType::Wall.tiles();
        assert_eq!(wall_tiles.len(), 1);
        assert_eq!(wall_tiles[0], (0, 0, Terrain::BuildingWall));

        // Hut should have both wall and floor tiles
        let hut_tiles = BuildingType::Hut.tiles();
        let wall_count = hut_tiles.iter().filter(|(_, _, t)| *t == Terrain::BuildingWall).count();
        let floor_count = hut_tiles.iter().filter(|(_, _, t)| *t == Terrain::BuildingFloor).count();
        assert!(wall_count > 0, "hut should have wall tiles");
        assert!(floor_count > 0, "hut should have floor tiles");
    }

    #[test]
    fn winter_increases_hunger() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        let start = world.get::<&Creature>(e).unwrap().hunger;

        // Normal hunger rate (mult=1.0)
        system_hunger(&mut world, 1.0);
        let normal_hunger = world.get::<&Creature>(e).unwrap().hunger;
        let normal_increase = normal_hunger - start;

        // Reset and test with winter multiplier (1.8)
        world.get::<&mut Creature>(e).unwrap().hunger = start;
        system_hunger(&mut world, 1.8);
        let winter_hunger = world.get::<&Creature>(e).unwrap().hunger;
        let winter_increase = winter_hunger - start;

        assert!(winter_increase > normal_increase,
            "winter hunger increase ({}) should exceed normal ({})", winter_increase, normal_increase);
    }

    #[test]
    fn wolf_attacks_villager_in_winter() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let wolf = spawn_predator(&mut world, 10.0, 10.0);
        let _villager = spawn_villager(&mut world, 15.0, 10.0);

        // Set wolf to very hungry
        world.get::<&mut Creature>(wolf).unwrap().hunger = 0.9;

        // With low aggression threshold (winter: 0.8), wolf should target villagers
        // since hunger (0.9) > threshold (0.8)
        for _ in 0..5 {
            system_ai(&mut world, &map, 0.8, 0);
            system_movement(&mut world, &map);
        }

        let state = world.get::<&Behavior>(wolf).unwrap().state;
        assert!(matches!(state, BehaviorState::Hunting { .. }),
            "hungry wolf should hunt villager in winter, got {:?}", state);
    }

    #[test]
    fn starvation_kills_creature() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);

        // Set hunger to max
        world.get::<&mut Creature>(e).unwrap().hunger = 1.0;

        let dead = system_death(&mut world);
        assert_eq!(dead.len(), 1, "one creature should die");
        assert!(world.get::<&Creature>(e).is_err(), "dead creature should be despawned");
    }

    #[test]
    fn prey_breeds_in_spring() {
        let mut world = World::new();
        // Spawn 1 prey at home with low hunger
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
        world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::AtHome { timer: 100 };

        let initial_count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Prey).count();
        assert_eq!(initial_count, 1);

        // Run many ticks to overcome 1/500 probability
        for _ in 0..5000 {
            system_breeding(&mut world, Season::Spring);
        }

        let final_count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Prey).count();
        assert!(final_count > 1, "prey should have bred in spring, count={}", final_count);
    }

    #[test]
    fn predator_breeds_when_fed() {
        let mut world = World::new();
        let e = spawn_predator(&mut world, 15.0, 15.0);
        world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
        world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::Wander { timer: 50 };

        let initial_count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Predator).count();
        assert_eq!(initial_count, 1);

        for _ in 0..10000 {
            system_breeding(&mut world, Season::Summer);
        }

        let final_count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Predator).count();
        assert!(final_count > 1, "wolf should have bred when well-fed, count={}", final_count);
    }

    #[test]
    fn no_breeding_in_winter() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
        world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::AtHome { timer: 100 };

        for _ in 0..5000 {
            system_breeding(&mut world, Season::Winter);
        }

        let count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Prey).count();
        assert_eq!(count, 1, "no breeding should occur in winter");
    }

    #[test]
    fn prey_population_capped_per_den() {
        let mut world = World::new();
        // Spawn 3 prey at the same den (already at cap)
        for _ in 0..3 {
            let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
            world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
            world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::AtHome { timer: 100 };
        }

        for _ in 0..5000 {
            system_breeding(&mut world, Season::Spring);
        }

        let count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Prey).count();
        assert_eq!(count, 3, "prey should be capped at 3 per den, got {}", count);
    }

    #[test]
    fn farm_produces_food() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);

        let mut total_food = 0u32;
        // Run enough ticks for at least one harvest (growth rate 0.003 in summer,
        // needs ~334 ticks to reach 1.0, then one more tick to harvest)
        for _ in 0..400 {
            total_food += system_farms(&mut world, Season::Summer);
        }
        assert!(total_food >= 3, "farm should have produced at least 3 food, got {}", total_food);
    }

    #[test]
    fn farm_no_growth_in_winter() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);

        for _ in 0..500 {
            system_farms(&mut world, Season::Winter);
        }

        let growth = world.query::<&FarmPlot>().iter().next().unwrap().growth;
        assert_eq!(growth, 0.0, "farm should not grow in winter, got {}", growth);
    }

    #[test]
    fn farm_visual_changes_with_growth() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);

        // Initially dirt
        let ch = world.query::<&Sprite>().iter()
            .find(|s| s.ch == '·')
            .map(|s| s.ch);
        assert_eq!(ch, Some('·'), "new farm should show dirt sprite");

        // Grow to medium (0.3+): run 150 ticks at summer rate 0.003 => 0.45
        for _ in 0..150 {
            system_farms(&mut world, Season::Summer);
        }
        {
            let mut q = world.query::<(&FarmPlot, &Sprite)>();
            let (_, sprite) = q.iter().next().unwrap();
            assert_eq!(sprite.ch, '♠', "mid-growth farm should show growing sprite, got '{}'", sprite.ch);
        }

        // Grow to mature (0.7+): run 100 more ticks => 0.75
        for _ in 0..100 {
            system_farms(&mut world, Season::Summer);
        }
        {
            let mut q = world.query::<(&FarmPlot, &Sprite)>();
            let (_, sprite) = q.iter().next().unwrap();
            assert_eq!(sprite.ch, '"', "mature farm should show mature sprite, got '{}'", sprite.ch);
        }
    }

    #[test]
    fn villager_settlement_survival() {
        // Simulate a mini settlement: 3 villagers, 2 berry bushes, 1 stockpile, Forest tiles
        let mut world = World::new();
        let mut map = walkable_map(40, 40);
        // Add some forest tiles for wood gathering
        for y in 5..10 {
            for x in 5..10 {
                map.set(x, y, Terrain::Forest);
            }
        }

        spawn_stockpile(&mut world, 20.0, 20.0);
        spawn_berry_bush(&mut world, 19.0, 19.0);
        spawn_berry_bush(&mut world, 21.0, 21.0);
        let v1 = spawn_villager(&mut world, 20.0, 21.0);
        let v2 = spawn_villager(&mut world, 21.0, 20.0);
        let v3 = spawn_villager(&mut world, 19.0, 20.0);

        let mut deposits = Vec::new();
        let mut any_ate = false;

        for tick in 0..3000 {
            system_hunger(&mut world, 1.0);
            let r = system_ai(&mut world, &map, 0.4, 0);
            deposits.extend(r.deposited);
            system_movement(&mut world, &map);
            system_death(&mut world);

            // Track states
            for (creature, behavior) in world.query::<(&Creature, &Behavior)>().iter() {
                if creature.species == Species::Villager {
                    if matches!(behavior.state, BehaviorState::Eating { .. }) {
                        any_ate = true;
                    }
                }
            }

            if tick % 500 == 0 {
                let alive = world.query::<&Creature>().iter()
                    .filter(|c| c.species == Species::Villager).count();
                let hungers: Vec<f64> = world.query::<&Creature>().iter()
                    .filter(|c| c.species == Species::Villager)
                    .map(|c| c.hunger)
                    .collect();
                let states: Vec<String> = world.query::<(&Creature, &Behavior)>().iter()
                    .filter(|(c, _)| c.species == Species::Villager)
                    .map(|(_, b)| format!("{:?}", b.state).split('{').next().unwrap_or("?").split('(').next().unwrap_or("?").trim().to_string())
                    .collect();
                eprintln!("tick {}: alive={} hunger={:?} states={:?} deposits={}",
                    tick, alive, hungers, states, deposits.len());
            }
        }

        let final_alive = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();

        eprintln!("Final: alive={}, total_deposits={}, any_ate={}", final_alive, deposits.len(), any_ate);
        assert!(any_ate, "villagers should eat at berry bushes");
        assert!(final_alive >= 2, "at least 2 villagers should survive 3000 ticks, got {}", final_alive);
    }

    #[test]
    fn villager_eats_from_stockpile_when_no_berries() {
        let mut world = World::new();
        // No food sources on this map — only stockpile
        let map = walkable_map(30, 30);

        let villager = spawn_villager(&mut world, 5.0, 5.0);
        spawn_stockpile(&mut world, 5.0, 5.0); // stockpile right next to villager

        // Hungry villager, wandering
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.6;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        // Pass stockpile_food=10 so villager knows food is available
        let result = system_ai(&mut world, &map, 0.4, 10);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(matches!(state, BehaviorState::Eating { .. }),
            "hungry villager near stockpile with food should eat, got: {:?}", state);
        assert_eq!(result.food_consumed, 1, "should consume 1 food from stockpile");
    }

    #[test]
    fn villager_gathers_stone_from_deposit() {
        let mut world = World::new();
        // Map with no mountain tiles — only stone deposits available
        let map = walkable_map(30, 30);

        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);
        spawn_stone_deposit(&mut world, 11.0, 10.0);

        // Low hunger, wandering — no forest nearby, should seek stone deposit
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        system_ai(&mut world, &map, 0.4, 0);

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(matches!(state, BehaviorState::Gathering { resource_type: ResourceType::Stone, .. }),
            "villager near stone deposit with low hunger should gather stone, got: {:?}", state);
    }
}
