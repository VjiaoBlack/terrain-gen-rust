use hecs::Entity;

use super::components::*;
use crate::tilemap::{Terrain, TileMap};

pub(super) fn dist(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

pub(super) fn move_toward(pos: &Position, tx: f64, ty: f64, speed: f64, vel: &mut Velocity) -> f64 {
    let dx = tx - pos.x;
    let dy = ty - pos.y;
    let d = dist(pos.x, pos.y, tx, ty);
    if d > 0.1 {
        vel.dx = (dx / d) * speed;
        vel.dy = (dy / d) * speed;
    }
    d
}

/// Move toward target using A* pathfinding. Falls back to direct movement if no path.
pub(super) fn move_toward_astar(
    pos: &Position,
    tx: f64,
    ty: f64,
    speed: f64,
    vel: &mut Velocity,
    map: &TileMap,
) -> f64 {
    let d = dist(pos.x, pos.y, tx, ty);
    if d < 0.5 {
        return d;
    }

    // Use A* for medium distances, direct for very short or very long
    if d > 1.5 && d < 80.0 {
        let budget = (d as usize * 4).min(600);
        if let Some((wx, wy)) = map.astar_next(pos.x, pos.y, tx, ty, budget) {
            let dx = wx - pos.x;
            let dy = wy - pos.y;
            let wd = (dx * dx + dy * dy).sqrt();
            if wd > 0.01 {
                vel.dx = (dx / wd) * speed;
                vel.dy = (dy / wd) * speed;
                return d;
            }
        }
    }

    // Fallback: direct movement
    move_toward(pos, tx, ty, speed, vel);
    d
}

pub(super) fn wander(
    pos: &Position,
    vel: &mut Velocity,
    speed: f64,
    map: &TileMap,
    rng: &mut impl rand::RngExt,
) {
    const DIRS: [(f64, f64); 8] = [
        (1.0, 0.0),
        (-1.0, 0.0),
        (0.0, 1.0),
        (0.0, -1.0),
        (1.0, 1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
    ];
    // Prefer outdoor tiles; only allow BuildingFloor if no outdoor option
    let mut outdoor: Vec<(f64, f64)> = Vec::new();
    let mut indoor: Vec<(f64, f64)> = Vec::new();
    for &(dx, dy) in &DIRS {
        let nx = pos.x + dx * 2.0;
        let ny = pos.y + dy * 2.0;
        if map.is_walkable(nx, ny) {
            if map.get(nx.round() as usize, ny.round() as usize) == Some(&Terrain::BuildingFloor) {
                indoor.push((dx, dy));
            } else {
                outdoor.push((dx, dy));
            }
        }
    }
    let candidates = if outdoor.is_empty() {
        &indoor
    } else {
        &outdoor
    };
    if let Some(&(dx, dy)) = candidates.get(rng.random_range(0..candidates.len().max(1))) {
        let len: f64 = (dx * dx + dy * dy).sqrt();
        vel.dx = dx / len * speed;
        vel.dy = dy / len * speed;
    } else {
        vel.dx = 0.0;
        vel.dy = 0.0;
    }
}

pub(super) fn do_wander_tick(
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
        BehaviorState::Seek {
            target_x, target_y, ..
        } => {
            let d = move_toward(pos, *target_x, *target_y, behavior.speed, vel);
            if d < 1.5 {
                vel.dx = 0.0;
                vel.dy = 0.0;
                // Longer cooldown to prevent seek→idle→seek loops
                behavior.state = BehaviorState::Idle {
                    timer: rng.random_range(60..150),
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
        BehaviorState::Exploring {
            target_x,
            target_y,
            timer,
        } => {
            if *timer == 0 {
                behavior.state = BehaviorState::Idle {
                    timer: rng.random_range(20..60),
                };
            } else {
                let d = move_toward(pos, *target_x, *target_y, behavior.speed, vel);
                if d < 2.0 {
                    // Arrived at frontier — idle and let AI re-evaluate
                    behavior.state = BehaviorState::Idle {
                        timer: rng.random_range(30..90),
                    };
                } else {
                    *timer -= 1;
                }
            }
        }
        BehaviorState::Gathering { .. }
        | BehaviorState::Hauling { .. }
        | BehaviorState::Sleeping { .. }
        | BehaviorState::Building { .. }
        | BehaviorState::Farming { .. }
        | BehaviorState::Working { .. } => {
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

/// Find the nearest tile of a given terrain type within a radius.
/// For walkable terrain (e.g. Forest), returns the tile position directly.
/// For non-walkable terrain (e.g. Mountain), returns an adjacent walkable tile.
pub(super) fn find_nearest_terrain(
    pos: &Position,
    map: &TileMap,
    terrain: Terrain,
    radius: f64,
) -> Option<(f64, f64)> {
    let cx = pos.x.round() as i32;
    let cy = pos.y.round() as i32;
    let r = radius as i32;
    let mut best: Option<(f64, f64, f64)> = None;

    let terrain_walkable = terrain.is_walkable();

    for dy in -r..=r {
        for dx in -r..=r {
            let tx = cx + dx;
            let ty = cy + dy;
            if tx >= 0
                && ty >= 0
                && let Some(t) = map.get(tx as usize, ty as usize)
                && *t == terrain
            {
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
    best.map(|(x, y, _)| (x, y))
}

/// Prey AI: eat berries, flee predators, return home.
pub(super) fn ai_prey(
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
                (BehaviorState::FleeHome { timer: 120 }, 0.0, 0.0, hunger)
            } else if *timer == 0 || hunger <= 0.0 {
                (BehaviorState::FleeHome { timer: 120 }, 0.0, 0.0, hunger)
            } else {
                (BehaviorState::Eating { timer: timer - 1 }, 0.0, 0.0, hunger)
            }
        }
        BehaviorState::FleeHome { timer } => {
            if *timer == 0 {
                // Give up fleeing, go idle
                return (
                    BehaviorState::Idle {
                        timer: rng.random_range(20..60),
                    },
                    0.0,
                    0.0,
                    hunger,
                );
            }
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward(pos, creature.home_x, creature.home_y, speed * 1.5, &mut vel);
            if d < 1.5 {
                (
                    BehaviorState::AtHome {
                        timer: rng.random_range(60..180),
                    },
                    0.0,
                    0.0,
                    hunger,
                )
            } else {
                (
                    BehaviorState::FleeHome { timer: timer - 1 },
                    vel.dx,
                    vel.dy,
                    hunger,
                )
            }
        }
        _ => {
            // Wander/Seek/Idle — check for threats and food
            if predator_nearby {
                return (BehaviorState::FleeHome { timer: 120 }, 0.0, 0.0, hunger);
            }
            if hunger > 0.5 {
                let nearest = food
                    .iter()
                    .map(|&(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((fx, fy, d)) = nearest {
                    if d < 1.5 {
                        return (
                            BehaviorState::Eating {
                                timer: rng.random_range(30..60),
                            },
                            0.0,
                            0.0,
                            hunger,
                        );
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward(pos, fx, fy, speed, &mut vel);
                        return (
                            BehaviorState::Seek {
                                target_x: fx,
                                target_y: fy,
                                reason: SeekReason::Food,
                            },
                            vel.dx,
                            vel.dy,
                            hunger,
                        );
                    }
                }
            }
            // Default: wander
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let mut bhv = Behavior {
                state: *state,
                speed,
            };
            do_wander_tick(&pos_copy, &mut vel, &mut bhv, map, rng);
            (bhv.state, vel.dx, vel.dy, hunger)
        }
    }
}

/// Predator AI: hunt visible prey, wander when not hungry.
/// Returns (new_state, vx, vy, hunger, Option<killed_prey_entity>).
pub(super) fn ai_predator(
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
                let victim = targets
                    .iter()
                    .map(|&(e, px, py, _)| (e, dist(pos.x, pos.y, px, py)))
                    .filter(|(_, d)| *d < 3.0)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                    .map(|(e, _)| e);
                (
                    BehaviorState::Wander { timer: 0 },
                    0.0,
                    0.0,
                    new_hunger,
                    victim,
                )
            } else {
                (
                    BehaviorState::Eating { timer: timer - 1 },
                    0.0,
                    0.0,
                    new_hunger,
                    None,
                )
            }
        }
        BehaviorState::Hunting { .. } => {
            // Find nearest visible target to chase (refreshes target each tick)
            let nearest = targets
                .iter()
                .filter(|(_, _, _, at_home)| !at_home)
                .map(|&(e, px, py, _)| (e, px, py, dist(pos.x, pos.y, px, py)))
                .filter(|(_, _, _, d)| *d < creature.sight_range)
                .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap());

            if let Some((target_e, px, py, d)) = nearest {
                if d < 2.0 {
                    // Caught target! Mark it as captured, start eating
                    return (
                        BehaviorState::Eating {
                            timer: rng.random_range(40..80),
                        },
                        0.0,
                        0.0,
                        hunger,
                        Some(target_e),
                    );
                }
                // Keep chasing
                let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                move_toward(pos, px, py, speed * 1.3, &mut vel);
                (
                    BehaviorState::Hunting {
                        target_x: px,
                        target_y: py,
                    },
                    vel.dx,
                    vel.dy,
                    hunger,
                    None,
                )
            } else {
                // Lost sight of all targets — give up
                (BehaviorState::Wander { timer: 0 }, 0.0, 0.0, hunger, None)
            }
        }
        _ => {
            if hunger > 0.4 {
                let nearest = targets
                    .iter()
                    .filter(|(_, _, _, at_home)| !at_home)
                    .map(|&(_, px, py, _)| (px, py, dist(pos.x, pos.y, px, py)))
                    .filter(|(_, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((px, py, _)) = nearest {
                    let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                    move_toward(pos, px, py, speed * 1.3, &mut vel);
                    return (
                        BehaviorState::Hunting {
                            target_x: px,
                            target_y: py,
                        },
                        vel.dx,
                        vel.dy,
                        hunger,
                        None,
                    );
                }
            }
            // Default: wander
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let mut bhv = Behavior {
                state: *state,
                speed,
            };
            do_wander_tick(&pos_copy, &mut vel, &mut bhv, map, rng);
            (bhv.state, vel.dx, vel.dy, hunger, None)
        }
    }
}

/// Villager AI: gather resources, eat when hungry, flee predators, build.
/// Returns (new_state, vx, vy, hunger, Option<ResourceType> deposited, Option<Entity> claimed_build_site).
pub(super) fn ai_villager(
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
    stockpile_food: u32,
    stockpile_wood: u32,
    stockpile_stone: u32,
    map: &TileMap,
    skill_mults: &SkillMults,
    rng: &mut impl rand::RngExt,
    hut_positions: &[(f64, f64)],
    is_night: bool,
) -> (
    BehaviorState,
    f64,
    f64,
    f64,
    Option<ResourceType>,
    Option<Entity>,
) {
    let mut hunger = creature.hunger;
    let pos_copy = *pos;

    match state {
        BehaviorState::Eating { timer } => {
            hunger = (hunger - 0.01).max(0.0);
            if predator_nearby {
                (
                    BehaviorState::FleeHome { timer: 120 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            } else if *timer == 0 || hunger <= 0.0 {
                (
                    BehaviorState::Idle {
                        timer: rng.random_range(20..60),
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            } else {
                (
                    BehaviorState::Eating { timer: timer - 1 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::FleeHome { timer } => {
            if *timer == 0 {
                // Give up fleeing, go idle
                return (
                    BehaviorState::Idle {
                        timer: rng.random_range(20..60),
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            // Flee toward nearest stockpile (or home)
            let (hx, hy) = stockpile
                .iter()
                .map(|&(sx, sy)| (sx, sy, dist(pos.x, pos.y, sx, sy)))
                .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
                .map(|(sx, sy, _)| (sx, sy))
                .unwrap_or((creature.home_x, creature.home_y));

            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward_astar(pos, hx, hy, speed * 1.5, &mut vel, map);
            if d < 1.5 {
                (
                    BehaviorState::Idle {
                        timer: rng.random_range(30..90),
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            } else {
                (
                    BehaviorState::FleeHome { timer: timer - 1 },
                    vel.dx,
                    vel.dy,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::Gathering {
            timer,
            resource_type,
        } => {
            if predator_nearby {
                return (
                    BehaviorState::FleeHome { timer: 120 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            if *timer == 0 {
                // Done gathering — haul to nearest stockpile
                let (hx, hy) = stockpile
                    .iter()
                    .map(|&(sx, sy)| (sx, sy, dist(pos.x, pos.y, sx, sy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
                    .map(|(sx, sy, _)| (sx, sy))
                    .unwrap_or((creature.home_x, creature.home_y));
                let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                move_toward_astar(pos, hx, hy, speed, &mut vel, map);
                (
                    BehaviorState::Hauling {
                        target_x: hx,
                        target_y: hy,
                        resource_type: *resource_type,
                    },
                    vel.dx,
                    vel.dy,
                    hunger,
                    None,
                    None,
                )
            } else {
                (
                    BehaviorState::Gathering {
                        timer: timer - 1,
                        resource_type: *resource_type,
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::Hauling {
            target_x,
            target_y,
            resource_type,
        } => {
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let d = move_toward_astar(pos, *target_x, *target_y, speed, &mut vel, map);
            if d < 1.5 {
                // Deposited resource at stockpile — short idle then re-evaluate
                (
                    BehaviorState::Idle {
                        timer: rng.random_range(5..15),
                    },
                    0.0,
                    0.0,
                    hunger,
                    Some(*resource_type),
                    None,
                )
            } else {
                (
                    BehaviorState::Hauling {
                        target_x: *target_x,
                        target_y: *target_y,
                        resource_type: *resource_type,
                    },
                    vel.dx,
                    vel.dy,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::Sleeping { timer } => {
            if *timer == 0 {
                (
                    BehaviorState::Idle { timer: 10 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            } else {
                (
                    BehaviorState::Sleeping { timer: timer - 1 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::Building {
            target_x,
            target_y,
            timer,
        } => {
            if predator_nearby {
                return (
                    BehaviorState::FleeHome { timer: 120 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            if *timer == 0 {
                // Done building this round — short pause then re-evaluate
                (
                    BehaviorState::Idle {
                        timer: rng.random_range(5..15),
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            } else {
                (
                    BehaviorState::Building {
                        target_x: *target_x,
                        target_y: *target_y,
                        timer: timer - 1,
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::Farming {
            target_x,
            target_y,
            lease,
        } => {
            // Lease expired → go idle, re-evaluate tasks
            if *lease == 0 {
                return (
                    BehaviorState::Idle { timer: 10 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            if predator_nearby {
                return (
                    BehaviorState::FleeHome { timer: 120 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            // Stop farming to gather resources if stockpile is critically low
            if stockpile_wood < 5 || stockpile_stone < 5 {
                return (
                    BehaviorState::Idle { timer: 5 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            if hunger > 0.6 {
                // Too hungry to farm — go eat
                return (
                    BehaviorState::Idle { timer: 5 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            let d = dist(pos.x, pos.y, *target_x, *target_y);
            if d > 2.5 {
                let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                move_toward_astar(pos, *target_x, *target_y, speed, &mut vel, map);
                (
                    BehaviorState::Farming {
                        target_x: *target_x,
                        target_y: *target_y,
                        lease: lease - 1,
                    },
                    vel.dx,
                    vel.dy,
                    hunger,
                    None,
                    None,
                )
            } else {
                // At farm — worker_present set by system_farm_workers after AI loop
                (
                    BehaviorState::Farming {
                        target_x: *target_x,
                        target_y: *target_y,
                        lease: lease - 1,
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            }
        }
        BehaviorState::Working {
            target_x,
            target_y,
            lease,
        } => {
            if *lease == 0 {
                return (
                    BehaviorState::Idle { timer: 10 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            if predator_nearby {
                return (
                    BehaviorState::FleeHome { timer: 120 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            if hunger > 0.6 {
                return (
                    BehaviorState::Idle { timer: 5 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }
            let d = dist(pos.x, pos.y, *target_x, *target_y);
            if d > 2.5 {
                let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                move_toward_astar(pos, *target_x, *target_y, speed, &mut vel, map);
                (
                    BehaviorState::Working {
                        target_x: *target_x,
                        target_y: *target_y,
                        lease: lease - 1,
                    },
                    vel.dx,
                    vel.dy,
                    hunger,
                    None,
                    None,
                )
            } else {
                // At workshop — worker_present set by system_workshop_workers after AI loop
                (
                    BehaviorState::Working {
                        target_x: *target_x,
                        target_y: *target_y,
                        lease: lease - 1,
                    },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                )
            }
        }
        _ => {
            // If villager is stuck inside a building (on BuildingFloor), try to leave
            let on_building = map.get(pos.x.round() as usize, pos.y.round() as usize)
                == Some(&Terrain::BuildingFloor);
            if on_building {
                // Find nearest outdoor (non-building) walkable tile
                for r in 1..=5i32 {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if dx.abs() != r && dy.abs() != r {
                                continue;
                            }
                            let nx = pos.x + dx as f64;
                            let ny = pos.y + dy as f64;
                            if map.is_walkable(nx, ny) {
                                let t = map.get(nx.round() as usize, ny.round() as usize);
                                if t != Some(&Terrain::BuildingFloor)
                                    && t != Some(&Terrain::BuildingWall)
                                {
                                    let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                                    move_toward_astar(pos, nx, ny, speed, &mut vel, map);
                                    return (
                                        BehaviorState::Seek {
                                            target_x: nx,
                                            target_y: ny,
                                            reason: SeekReason::ExitBuilding,
                                        },
                                        vel.dx,
                                        vel.dy,
                                        hunger,
                                        None,
                                        None,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Night shelter-seeking: villagers look for huts to sleep in at night
            if is_night {
                let nearest_hut = hut_positions
                    .iter()
                    .map(|&(hx, hy)| (hx, hy, dist(pos.x, pos.y, hx, hy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((hx, hy, d)) = nearest_hut {
                    if d < 1.5 {
                        return (
                            BehaviorState::Sleeping { timer: 200 },
                            0.0,
                            0.0,
                            hunger,
                            None,
                            None,
                        );
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward_astar(pos, hx, hy, speed, &mut vel, map);
                        return (
                            BehaviorState::Seek {
                                target_x: hx,
                                target_y: hy,
                                reason: SeekReason::Hut,
                            },
                            vel.dx,
                            vel.dy,
                            hunger,
                            None,
                            None,
                        );
                    }
                } else {
                    // No hut available — sleep outdoors (shorter rest)
                    return (
                        BehaviorState::Sleeping { timer: 100 },
                        0.0,
                        0.0,
                        hunger,
                        None,
                        None,
                    );
                }
            }

            // Wander/Seek/Idle — check for threats, food, and gathering
            if predator_nearby {
                return (
                    BehaviorState::FleeHome { timer: 120 },
                    0.0,
                    0.0,
                    hunger,
                    None,
                    None,
                );
            }

            // Eat: if hungry and near food (eat early to avoid starvation)
            if hunger > 0.4 {
                let nearest_food = food
                    .iter()
                    .map(|&(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)))
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((fx, fy, d)) = nearest_food {
                    if d < 1.5 {
                        return (
                            BehaviorState::Eating {
                                timer: rng.random_range(30..60),
                            },
                            0.0,
                            0.0,
                            hunger,
                            None,
                            None,
                        );
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward_astar(pos, fx, fy, speed, &mut vel, map);
                        return (
                            BehaviorState::Seek {
                                target_x: fx,
                                target_y: fy,
                                reason: SeekReason::Food,
                            },
                            vel.dx,
                            vel.dy,
                            hunger,
                            None,
                            None,
                        );
                    }
                }
                // No berry bush reachable — eat from stockpile if food available
                if has_stockpile_food {
                    let nearest_stockpile = stockpile
                        .iter()
                        .map(|&(sx, sy)| (sx, sy, dist(pos.x, pos.y, sx, sy)))
                        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                    if let Some((sx, sy, d)) = nearest_stockpile {
                        if d < 1.5 {
                            return (
                                BehaviorState::Eating {
                                    timer: rng.random_range(20..40),
                                },
                                0.0,
                                0.0,
                                hunger,
                                None,
                                None,
                            );
                        } else {
                            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                            move_toward_astar(pos, sx, sy, speed, &mut vel, map);
                            return (
                                BehaviorState::Seek {
                                    target_x: sx,
                                    target_y: sy,
                                    reason: SeekReason::Stockpile,
                                },
                                vel.dx,
                                vel.dy,
                                hunger,
                                None,
                                None,
                            );
                        }
                    }
                }
            }

            // Scarcity-driven task selection: score urgency of build vs gather
            // Food gathering gets urgent when stockpile is low relative to what villagers eat
            let food_urgent = stockpile_food < 5 || (has_stockpile_food && stockpile_food < 10);
            let build_available = hunger < 0.4
                && build_sites
                    .iter()
                    .any(|&(_, bx, by, _)| dist(pos.x, pos.y, bx, by) < creature.sight_range);

            // When food is critically low, skip building and gather food/resources instead
            // (unless the build site IS a farm — always prioritize farm construction)
            let should_build = if build_available && hunger < 0.4 {
                if food_urgent {
                    // Only build farms when food is urgent
                    build_sites.iter().any(|&(_, bx, by, assigned)| {
                        !assigned && dist(pos.x, pos.y, bx, by) < creature.sight_range
                    })
                } else {
                    true
                }
            } else {
                false
            };

            if should_build {
                let nearest_site = build_sites
                    .iter()
                    .map(|&(e, bx, by, _)| (e, bx, by, dist(pos.x, pos.y, bx, by)))
                    .filter(|(_, _, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap());
                if let Some((site_e, bx, by, d)) = nearest_site {
                    if d < 1.5 {
                        return (
                            BehaviorState::Building {
                                target_x: bx,
                                target_y: by,
                                timer: 30,
                            },
                            0.0,
                            0.0,
                            hunger,
                            None,
                            Some(site_e),
                        );
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward_astar(pos, bx, by, speed, &mut vel, map);
                        return (
                            BehaviorState::Seek {
                                target_x: bx,
                                target_y: by,
                                reason: SeekReason::BuildSite,
                            },
                            vel.dx,
                            vel.dy,
                            hunger,
                            None,
                            Some(site_e),
                        );
                    }
                }
            }

            // When food stockpile is critically low, seek food sources even if not personally hungry
            if food_urgent && hunger < 0.3 {
                let nearest_food = food
                    .iter()
                    .map(|&(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)))
                    .filter(|(_, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                if let Some((fx, fy, d)) = nearest_food {
                    if d < 1.5 {
                        return (
                            BehaviorState::Gathering {
                                timer: 60,
                                resource_type: ResourceType::Food,
                            },
                            0.0,
                            0.0,
                            hunger,
                            None,
                            None,
                        );
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        move_toward_astar(pos, fx, fy, speed, &mut vel, map);
                        return (
                            BehaviorState::Seek {
                                target_x: fx,
                                target_y: fy,
                                reason: SeekReason::Food,
                            },
                            vel.dx,
                            vel.dy,
                            hunger,
                            None,
                            None,
                        );
                    }
                }
            }

            // Gather resources: pick whichever resource is most needed
            if hunger < 0.4 {
                let wood_target =
                    find_nearest_terrain(pos, map, Terrain::Forest, creature.sight_range)
                        .map(|(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)));

                let nearest_deposit = stone_deposits
                    .iter()
                    .map(|&(dx, dy)| (dx, dy, dist(pos.x, pos.y, dx, dy)))
                    .filter(|(_, _, d)| *d < creature.sight_range)
                    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                let stone_target = nearest_deposit.or_else(|| {
                    find_nearest_terrain(pos, map, Terrain::Mountain, creature.sight_range)
                        .map(|(mx, my)| (mx, my, dist(pos.x, pos.y, mx, my)))
                });

                // Decide which to gather: bias toward whichever stockpile is lower.
                // If one resource has less than half the other, strongly prefer it.
                // Otherwise fall back to distance.
                let gather_wood_first = match (&wood_target, &stone_target) {
                    (Some((_, _, wd)), Some((_, _, sd))) => {
                        if stockpile_stone < stockpile_wood / 2 {
                            false // stone is critically low
                        } else if stockpile_wood < stockpile_stone / 2 {
                            true // wood is critically low
                        } else if stockpile_stone < stockpile_wood {
                            false // stone is lower, prefer it
                        } else if stockpile_wood < stockpile_stone {
                            true // wood is lower, prefer it
                        } else {
                            *wd <= *sd // equal stockpiles: go to closer one
                        }
                    }
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                    (None, None) => true,
                };

                let targets = if gather_wood_first {
                    [
                        wood_target.map(|(x, y, d)| (x, y, d, ResourceType::Wood)),
                        stone_target.map(|(x, y, d)| (x, y, d, ResourceType::Stone)),
                    ]
                } else {
                    [
                        stone_target.map(|(x, y, d)| (x, y, d, ResourceType::Stone)),
                        wood_target.map(|(x, y, d)| (x, y, d, ResourceType::Wood)),
                    ]
                };

                if let Some(target) = targets.iter().flatten().next() {
                    let (tx, ty, d, rt) = *target;
                    if d < 1.5 {
                        let timer = match rt {
                            ResourceType::Wood => (90.0 / skill_mults.gather_wood_speed) as u32,
                            ResourceType::Stone => (90.0 / skill_mults.gather_stone_speed) as u32,
                            _ => 90,
                        };
                        return (
                            BehaviorState::Gathering {
                                timer,
                                resource_type: rt,
                            },
                            0.0,
                            0.0,
                            hunger,
                            None,
                            None,
                        );
                    } else {
                        let mut vel = Velocity { dx: 0.0, dy: 0.0 };
                        let reason = match rt {
                            ResourceType::Wood => SeekReason::Wood,
                            ResourceType::Stone => SeekReason::Stone,
                            _ => SeekReason::Food,
                        };
                        move_toward_astar(pos, tx, ty, speed, &mut vel, map);
                        return (
                            BehaviorState::Seek {
                                target_x: tx,
                                target_y: ty,
                                reason,
                            },
                            vel.dx,
                            vel.dy,
                            hunger,
                            None,
                            None,
                        );
                    }
                }
            }

            // Default: wander
            let mut vel = Velocity { dx: 0.0, dy: 0.0 };
            let mut bhv = Behavior {
                state: *state,
                speed,
            };
            do_wander_tick(&pos_copy, &mut vel, &mut bhv, map, rng);
            (bhv.state, vel.dx, vel.dy, hunger, None, None)
        }
    }
}
