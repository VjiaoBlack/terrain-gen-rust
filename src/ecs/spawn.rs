use hecs::{Entity, World};
use rand::RngExt;

use super::components::*;
use crate::renderer::Color;

// --- Spawn Helpers ---

pub fn spawn_entity(
    world: &mut World,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    ch: char,
    fg: Color,
) -> Entity {
    world.spawn((Position { x, y }, Velocity { dx, dy }, Sprite { ch, fg }))
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
        Sprite {
            ch: 'r',
            fg: Color(180, 140, 80),
        }, // rabbit-colored
        Behavior {
            state: BehaviorState::AtHome { timer: 30 },
            speed: 0.18,
        },
        Creature {
            species: Species::Prey,
            hunger: 0.2,
            home_x,
            home_y,
            sight_range: 18.0,
        },
    ))
}

pub fn spawn_predator(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite {
            ch: 'W',
            fg: Color(160, 50, 50),
        }, // wolf-colored
        Behavior {
            state: BehaviorState::Wander { timer: 0 },
            speed: 0.22,
        },
        Creature {
            species: Species::Predator,
            hunger: 0.3,
            home_x: x,
            home_y: y,
            sight_range: 25.0,
        },
    ))
}

pub fn spawn_berry_bush(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '♦',
            fg: Color(200, 40, 80),
        }, // red berries
        FoodSource,
        ResourceYield {
            remaining: 20,
            max: 20,
        },
    ))
}

pub fn spawn_den(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: 'O',
            fg: Color(140, 100, 60),
        }, // burrow
        Den,
    ))
}

pub fn spawn_stone_deposit(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '●',
            fg: Color(150, 140, 130),
        }, // grey stone
        StoneDeposit,
        ResourceYield {
            remaining: 20,
            max: 20,
        },
    ))
}

pub fn spawn_villager(world: &mut World, x: f64, y: f64) -> Entity {
    let mut memory = VillagerMemory::new();
    memory.home = Some((x, y));
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite {
            ch: 'V',
            fg: Color(100, 200, 255),
        }, // villager: light blue
        Behavior {
            state: BehaviorState::Idle { timer: 10 },
            speed: 0.15,
        },
        Creature {
            species: Species::Villager,
            hunger: 0.1,
            home_x: x,
            home_y: y,
            sight_range: 22.0,
        },
        PathCache::default(),
        TickSchedule::default(), // next_ai_tick: 0 → runs immediately on first tick
        memory,
    ))
}

/// Spawn a villager with a staggered initial AI tick to prevent all villagers
/// from evaluating on the same tick. Used in production; tests use `spawn_villager`.
pub fn spawn_villager_staggered(world: &mut World, x: f64, y: f64, current_tick: u64) -> Entity {
    let mut rng = rand::rng();
    let offset: u64 = rng.random_range(0..8);
    let e = spawn_villager(world, x, y);
    if let Ok(mut schedule) = world.get::<&mut TickSchedule>(e) {
        schedule.next_ai_tick = current_tick + offset;
    }
    e
}

pub fn spawn_build_site(
    world: &mut World,
    x: f64,
    y: f64,
    building_type: BuildingType,
    queued_at: u64,
) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '#',
            fg: Color(200, 180, 100),
        },
        BuildSite {
            building_type,
            progress: 0,
            required: building_type.build_time(),
            assigned: false,
            queued_at,
        },
    ))
}

pub fn spawn_stockpile(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '■',
            fg: Color(180, 140, 60),
        }, // wooden stockpile
        Stockpile,
        BulletinBoard::default(),
    ))
}

pub fn spawn_farm_plot(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '·',
            fg: Color(120, 80, 30),
        }, // starts as dirt
        FarmPlot {
            growth: 0.0,
            harvest_ready: false,
            worker_present: false,
            pending_food: 0,
            tile_x: x as usize,
            tile_y: y as usize,
            fallow: false,
        },
    ))
}

pub fn spawn_garrison(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '⚔',
            fg: Color(180, 50, 50),
        },
        GarrisonBuilding { defense_bonus: 5.0 },
        GarrisonBoard::default(),
    ))
}

pub fn spawn_town_hall(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: 'H',
            fg: Color(255, 220, 60),
        },
        TownHallBuilding { housing_bonus: 20 },
    ))
}

pub fn spawn_hut(world: &mut World, x: f64, y: f64) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '⌂',
            fg: Color(160, 130, 90),
        },
        HutBuilding {
            capacity: 4,
            occupants: 0,
        },
    ))
}

pub fn spawn_processing_building(world: &mut World, x: f64, y: f64, recipe: Recipe) -> Entity {
    world.spawn((
        Position { x, y },
        Sprite {
            ch: '⚙',
            fg: Color(200, 180, 100),
        },
        ProcessingBuilding {
            recipe,
            progress: 0,
            required: 120,
            worker_present: false,
            material_needed: None,
        },
    ))
}
