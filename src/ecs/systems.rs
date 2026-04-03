use hecs::{Entity, World};
use rand::RngExt;

use super::ai::*;
use super::components::*;
use super::spatial::{SpatialHashGrid, category};
use super::spawn::*;
use crate::renderer::{Color, Renderer};
use crate::simulation::{MoistureMap, Season};
use crate::tilemap::{Terrain, TileMap};

/// Eight cardinal + diagonal directions for terrain sampling.
const EIGHT_DIRS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.7071, 0.7071),
    (-0.7071, 0.7071),
    (0.7071, -0.7071),
    (-0.7071, -0.7071),
];

// --- Systems ---

/// Move entities with terrain collision. Each axis is tested independently so
/// entities slide along walls. If blocked, velocity on that axis is reversed
/// (NPCs bounce).
pub fn system_movement(world: &mut World, map: &TileMap) {
    for (pos, vel) in world.query_mut::<(&mut Position, &mut Velocity)>() {
        // Apply terrain speed multiplier (e.g. roads give 1.5x bonus)
        let ix = pos.x.round() as i64;
        let iy = pos.y.round() as i64;
        let speed_mult = if ix >= 0 && iy >= 0 {
            map.get(ix as usize, iy as usize)
                .map(|t| t.speed_multiplier())
                .unwrap_or(1.0)
        } else {
            1.0
        };

        // Try X
        let new_x = pos.x + vel.dx * speed_mult;
        if map.is_walkable(new_x, pos.y) {
            pos.x = new_x;
        } else {
            vel.dx = -vel.dx; // bounce
        }
        // Try Y
        let new_y = pos.y + vel.dy * speed_mult;
        if map.is_walkable(pos.x, new_y) {
            pos.y = new_y;
        } else {
            vel.dy = -vel.dy; // bounce
        }
    }
}

/// Hunger increases each tick.
/// Rate: 0.0005/tick → full hunger in ~2000 ticks (~1.7 in-game days at 0.02h/tick).
/// Creatures should eat roughly once per day.
/// Also triggers hunger-critical interrupt: if hunger > 0.85, force AI next tick.
pub fn system_hunger(world: &mut World, hunger_mult: f64, current_tick: u64) {
    for (creature, schedule) in world.query_mut::<(&mut Creature, Option<&mut TickSchedule>)>() {
        let rate = match creature.species {
            Species::Prey => 0.0005,
            Species::Predator => 0.0006,  // predators burn slightly more
            Species::Villager => 0.00015, // villagers burn slowly — settlements need time to establish
        };
        creature.hunger = (creature.hunger + rate * hunger_mult).min(1.0);
        // Hunger-critical interrupt: force AI evaluation when starving
        if creature.hunger > 0.85 {
            if let Some(sched) = schedule {
                if sched.next_ai_tick > current_tick + 1 {
                    sched.next_ai_tick = current_tick + 1;
                }
            }
        }
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
pub fn system_ai(
    world: &mut World,
    map: &TileMap,
    grid: &SpatialHashGrid,
    wolf_aggression: f64,
    stockpile_food: u32,
    stockpile_wood: u32,
    stockpile_stone: u32,
    stockpile_grain: u32,
    stockpile_bread: u32,
    skill_mults: &SkillMults,
    settlement_defended: bool,
    is_night: bool,
    frontier: &[(usize, usize)],
    current_tick: u64,
) -> AiResult {
    let mut rng = rand::rng();
    let mut deposited_resources: Vec<ResourceType> = Vec::new();
    let mut food_consumed: u32 = 0;
    let mut grain_consumed: u32 = 0;
    let mut bread_consumed: u32 = 0;
    let mut farming_ticks: u32 = 0;
    let mut mining_ticks: u32 = 0;
    let mut woodcutting_ticks: u32 = 0;
    let mut building_ticks: u32 = 0;

    // Phase 1: snapshot world state — prey/villager need extra fields (entity, at_home/captured)
    // so we still query those from the World. Everything else uses the spatial grid.
    let prey_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
        .filter(|(_, _, c, _)| c.species == Species::Prey)
        .map(|(e, p, _, b)| {
            (
                e,
                p.x,
                p.y,
                matches!(
                    b.state,
                    BehaviorState::AtHome { .. } | BehaviorState::Captured
                ),
            )
        })
        .collect();

    let villager_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
        .filter(|(_, _, c, _)| c.species == Species::Villager)
        .map(|(e, p, _, b)| (e, p.x, p.y, matches!(b.state, BehaviorState::Captured)))
        .collect();

    let build_site_positions: Vec<(Entity, f64, f64, bool)> = world
        .query::<(Entity, &Position, &BuildSite)>()
        .iter()
        .map(|(e, pos, site)| (e, pos.x, pos.y, site.assigned))
        .collect();

    // Phase 0 (local awareness): compute StockpileFullness from global resource counts.
    // This is the data-path change — villager AI reads fullness tiers instead of raw u32.
    // For now the raw counts are still passed through; Phase 2 will remove them.
    let stockpile_fullness = StockpileState {
        food: StockpileFullness::from_count(stockpile_food),
        wood: StockpileFullness::from_count(stockpile_wood),
        stone: StockpileFullness::from_count(stockpile_stone),
    };

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
    let mut harvest_positions: Vec<(f64, f64, ResourceType)> = Vec::new(); // where harvests completed
    let mut wood_harvest_pos: Vec<(f64, f64)> = Vec::new(); // wood harvest positions for deforestation
    let mut stone_harvest_pos: Vec<(f64, f64)> = Vec::new(); // mountain mining positions for terrain changes
    for e in entities {
        // Read position (copy) and check if it's a creature
        let Some(pos) = world.get::<&Position>(e).ok().map(|p| *p) else {
            continue;
        };
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

        // Tick budgeting: skip villagers whose AI is not scheduled this tick.
        // Movement continues (velocity persists), only AI decisions are gated.
        if creature.species == Species::Villager {
            if let Ok(schedule) = world.get::<&TickSchedule>(e) {
                if schedule.next_ai_tick > current_tick {
                    continue;
                }
            }
        }

        // Decide the new state and velocity
        let (new_state, new_vx, new_vy, new_hunger, kill, deposited) = match creature.species {
            Species::Prey => {
                let predator_nearby =
                    grid.any_within(pos.x, pos.y, creature.sight_range, category::PREDATOR);

                let (s, vx, vy, h) = ai_prey(
                    &pos,
                    &creature,
                    &behavior_state,
                    speed,
                    predator_nearby,
                    grid,
                    map,
                    &mut rng,
                );
                (s, vx, vy, h, None, None)
            }
            Species::Predator => {
                let effective_aggression = if settlement_defended {
                    1.0 // wolves won't hunt villagers unless at max hunger
                } else {
                    wolf_aggression
                };
                let (s, vx, vy, h, k) = ai_predator(
                    &pos,
                    &creature,
                    &behavior_state,
                    speed,
                    &prey_positions,
                    &villager_positions,
                    effective_aggression,
                    map,
                    &mut rng,
                );
                (s, vx, vy, h, k, None)
            }
            Species::Villager => {
                // Villagers only flee wolves within close threat range (not full sight range)
                let threat_range = 8.0_f64.min(creature.sight_range);
                let predator_nearby =
                    grid.any_within(pos.x, pos.y, threat_range, category::PREDATOR);

                let remaining_grain = stockpile_grain.saturating_sub(grain_consumed);
                let remaining_food = stockpile_food.saturating_sub(food_consumed);
                let remaining_bread = stockpile_bread.saturating_sub(bread_consumed);
                let has_food = remaining_grain > 0 || remaining_food > 0 || remaining_bread > 0;
                let was_eating = matches!(behavior_state, BehaviorState::Eating { .. });
                let near_food_source = grid.any_within(pos.x, pos.y, 2.0, category::FOOD_SOURCE);

                // Get or create PathCache for this villager
                let mut path_cache = world
                    .get::<&PathCache>(e)
                    .ok()
                    .map(|c| (*c).clone())
                    .unwrap_or_default();

                let (s, vx, vy, h, dep, claim_site) = ai_villager(
                    &pos,
                    &creature,
                    &behavior_state,
                    speed,
                    predator_nearby,
                    grid,
                    &build_site_positions,
                    has_food,
                    remaining_food + remaining_grain + remaining_bread,
                    stockpile_wood,
                    stockpile_stone,
                    map,
                    skill_mults,
                    &mut rng,
                    is_night,
                    frontier,
                    &stockpile_fullness,
                    &mut path_cache,
                    current_tick,
                );

                // Write back PathCache
                if let Ok(mut cache) = world.get::<&mut PathCache>(e) {
                    *cache = path_cache;
                } else {
                    // Entity doesn't have PathCache yet (old save); add it
                    let _ = world.insert_one(e, path_cache);
                }

                // Villager just started eating near stockpile: grain → bread → food
                if matches!(s, BehaviorState::Eating { .. }) && !was_eating && !near_food_source {
                    if remaining_grain > 0 {
                        grain_consumed += 1;
                    } else if remaining_bread > 0 {
                        bread_consumed += 1;
                    } else {
                        food_consumed += 1;
                    }
                }
                // If villager claims a build site, mark it assigned
                if let Some(site_entity) = claim_site
                    && let Ok(mut site) = world.get::<&mut BuildSite>(site_entity)
                {
                    site.assigned = true;
                }
                // Track harvest completions for resource depletion
                if matches!(
                    behavior_state,
                    BehaviorState::Gathering { timer: 1, .. }
                        | BehaviorState::Gathering { timer: 0, .. }
                ) && let BehaviorState::Hauling { resource_type, .. } = s
                {
                    harvest_positions.push((pos.x, pos.y, resource_type));
                }
                (s, vx, vy, h, None, dep)
            }
        };

        if let Some(resource) = deposited {
            deposited_resources.push(resource);

            // Villager just deposited at stockpile — update their believed stockpile counts.
            // This is the "bulletin board read": beliefs refresh on visit.
            if creature.species == Species::Villager {
                // Compute what the stockpile will look like after all deposits so far this tick
                let mut dep_food = 0u32;
                let mut dep_wood = 0u32;
                let mut dep_stone = 0u32;
                for r in &deposited_resources {
                    match r {
                        ResourceType::Food => dep_food += 1,
                        ResourceType::Wood => dep_wood += 1,
                        ResourceType::Stone => dep_stone += 1,
                        _ => {}
                    }
                }
                let believed = BelievedStockpile {
                    food: stockpile_food.saturating_sub(food_consumed) + dep_food,
                    wood: stockpile_wood + dep_wood,
                    stone: stockpile_stone + dep_stone,
                    tick_observed: current_tick,
                };
                if let Ok(mut memory) = world.get::<&mut VillagerMemory>(e) {
                    memory.believed_stockpile = Some(believed);
                }
            }
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
        // Update tick schedule for villagers after AI evaluation
        if creature.species == Species::Villager {
            let interval = tick_priority(&new_state);
            if let Ok(mut schedule) = world.get::<&mut TickSchedule>(e) {
                schedule.interval = interval;
                schedule.next_ai_tick = current_tick + interval as u64;
            }
        }
        // Track build progress and activity for skills
        if creature.species == Species::Villager {
            match new_state {
                BehaviorState::Building {
                    target_x, target_y, ..
                } => {
                    build_progress.push((target_x, target_y));
                    building_ticks += 1;
                }
                BehaviorState::Gathering {
                    resource_type: ResourceType::Wood,
                    ..
                } => {
                    woodcutting_ticks += 1;
                }
                BehaviorState::Gathering {
                    resource_type: ResourceType::Stone,
                    ..
                } => {
                    mining_ticks += 1;
                }
                BehaviorState::Gathering {
                    resource_type: ResourceType::Food,
                    ..
                } => {
                    farming_ticks += 1;
                }
                _ => {}
            }
        } else if let BehaviorState::Building {
            target_x, target_y, ..
        } = new_state
        {
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

    // Predator proximity interrupt: force villagers near predators to run AI next tick.
    // Collect predator positions first, then scan villagers with TickSchedule.
    let predator_positions: Vec<(f64, f64)> = world
        .query::<(&Position, &Creature)>()
        .iter()
        .filter(|(_, c)| c.species == Species::Predator)
        .map(|(p, _)| (p.x, p.y))
        .collect();
    if !predator_positions.is_empty() {
        let threat_range = 8.0_f64;
        for (pos, schedule) in world.query_mut::<(&Position, &mut TickSchedule)>() {
            if schedule.next_ai_tick <= current_tick + 1 {
                continue; // already scheduled soon
            }
            for &(px, py) in &predator_positions {
                let dx = pos.x - px;
                let dy = pos.y - py;
                if dx * dx + dy * dy < threat_range * threat_range {
                    schedule.next_ai_tick = current_tick;
                    break;
                }
            }
        }
    }

    // Increment progress on build sites where villagers are working
    let build_bonus = 1 + skill_mults.build_speed;
    for (bx, by) in build_progress {
        for (pos, site) in world.query_mut::<(&Position, &mut BuildSite)>() {
            if (pos.x - bx).abs() < 1.5 && (pos.y - by).abs() < 1.5 {
                site.progress += build_bonus;
                break;
            }
        }
    }

    // Deplete resources at harvest positions
    let mut to_deplete_despawn: Vec<Entity> = Vec::new();
    for (hx, hy, rt) in harvest_positions {
        // Find nearest resource entity of matching type
        match rt {
            ResourceType::Food => {
                let mut best: Option<(Entity, f64)> = None;
                for (e, pos, _fs, _ry) in world
                    .query::<(Entity, &Position, &FoodSource, &mut ResourceYield)>()
                    .iter()
                {
                    let d = dist(pos.x, pos.y, hx, hy);
                    if d < 3.0 && best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                        best = Some((e, d));
                    }
                }
                if let Some((e, _)) = best
                    && let Ok(mut ry) = world.get::<&mut ResourceYield>(e)
                {
                    ry.remaining = ry.remaining.saturating_sub(1);
                    if ry.remaining == 0 {
                        to_deplete_despawn.push(e);
                    }
                }
            }
            ResourceType::Stone => {
                let mut best: Option<(Entity, f64)> = None;
                for (e, pos, _sd, _ry) in world
                    .query::<(Entity, &Position, &StoneDeposit, &mut ResourceYield)>()
                    .iter()
                {
                    let d = dist(pos.x, pos.y, hx, hy);
                    if d < 3.0 && best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                        best = Some((e, d));
                    }
                }
                if let Some((e, _)) = best
                    && let Ok(mut ry) = world.get::<&mut ResourceYield>(e)
                {
                    ry.remaining = ry.remaining.saturating_sub(1);
                    if ry.remaining == 0 {
                        to_deplete_despawn.push(e);
                    }
                } else {
                    // No StoneDeposit entity nearby — this was mountain mining
                    stone_harvest_pos.push((hx, hy));
                }
            }
            ResourceType::Wood => {
                wood_harvest_pos.push((hx, hy));
            }
            _ => {} // Refined resources not gathered from terrain
        }
    }
    let mut depleted_stone_positions: Vec<(f64, f64)> = Vec::new();
    for e in &to_deplete_despawn {
        // Record depleted stone deposit positions for ScarredGround conversion
        if world.get::<&StoneDeposit>(*e).is_ok() {
            if let Ok(pos) = world.get::<&Position>(*e) {
                depleted_stone_positions.push((pos.x, pos.y));
            }
        }
        let _ = world.despawn(*e);
    }

    AiResult {
        deposited: deposited_resources,
        food_consumed,
        grain_consumed,
        bread_consumed,
        farming_ticks,
        mining_ticks,
        woodcutting_ticks,
        building_ticks,
        wood_harvest_positions: wood_harvest_pos,
        stone_harvest_positions: stone_harvest_pos,
        depleted_stone_positions,
    }
}

/// Regrowth system: berry bushes regrow near trees, and deforested terrain recovers.
/// Lifecycle: Forest -> Stump -> Bare -> Sapling -> Forest.
/// Stone does NOT regrow.
pub fn system_regrowth(
    world: &mut World,
    map: &mut TileMap,
    vegetation: &crate::simulation::VegetationMap,
    tick: u64,
) {
    // Only check every 400 ticks
    if !tick.is_multiple_of(400) {
        return;
    }

    let mut rng = rand::rng();

    // Berry bush regrowth: small chance near forest tiles
    // Count existing bushes to cap total
    let bush_count = world.query::<&FoodSource>().iter().count();
    if bush_count < 30 {
        // Pick a few random forest tiles to maybe spawn a bush
        for _ in 0..5 {
            let x = rng.random_range(1..map.width.saturating_sub(1) as u32) as usize;
            let y = rng.random_range(1..map.height.saturating_sub(1) as u32) as usize;
            if map.get(x, y) == Some(&Terrain::Grass) {
                // Check if forest is nearby
                let near_forest = [(-1i32, 0), (1, 0), (0, -1), (0, 1)]
                    .iter()
                    .any(|&(dx, dy)| {
                        let nx = (x as i32 + dx) as usize;
                        let ny = (y as i32 + dy) as usize;
                        map.get(nx, ny) == Some(&Terrain::Forest)
                    });
                if near_forest && rng.random_range(0u32..100) < 3 {
                    spawn_berry_bush(world, x as f64, y as f64);
                }
            }
        }
    }

    // Deforestation regrowth: sample random tiles and advance lifecycle.
    // Sample 20 random tiles per check (scales well for 256x256 maps).
    let sample_count = 20usize;
    for _ in 0..sample_count {
        let x = rng.random_range(0..map.width as u32) as usize;
        let y = rng.random_range(0..map.height as u32) as usize;
        let Some(terrain) = map.get(x, y).copied() else {
            continue;
        };
        match terrain {
            // Stump -> Bare: 30% chance per 400-tick check
            Terrain::Stump => {
                if rng.random_range(0u32..100) < 30 {
                    map.set(x, y, Terrain::Bare);
                }
            }
            // Bare -> Sapling: requires adjacent Forest or Sapling, gated on vegetation > 0.2
            Terrain::Bare => {
                if vegetation.get(x, y) <= 0.2 {
                    continue;
                }
                let mut adj_forest = false;
                let mut adj_sapling = false;
                for &(dx, dy) in &[(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 {
                        match map.get(nx as usize, ny as usize) {
                            Some(&Terrain::Forest) => adj_forest = true,
                            Some(&Terrain::Sapling) => adj_sapling = true,
                            _ => {}
                        }
                    }
                }
                let chance = if adj_forest {
                    5u32
                } else if adj_sapling {
                    2
                } else {
                    0
                };
                if chance > 0 && rng.random_range(0u32..100) < chance {
                    map.set(x, y, Terrain::Sapling);
                }
            }
            // Sapling -> Forest: 3% chance per check
            Terrain::Sapling => {
                if rng.random_range(0u32..100) < 3 {
                    map.set(x, y, Terrain::Forest);
                }
            }
            _ => {}
        }
    }
}

/// Assign idle/wandering villagers to farms or workshops that need workers.
/// Priority: farms with pending food > farms needing tending > workshops with inputs.
pub fn system_assign_workers(world: &mut World, resources: &Resources) {
    // Collect farm positions and their state
    let farms: Vec<(f64, f64, bool, u32)> = world
        .query::<(&Position, &FarmPlot)>()
        .iter()
        .map(|(p, f)| {
            (
                p.x,
                p.y,
                f.harvest_ready || f.pending_food > 0,
                f.pending_food,
            )
        })
        .collect();

    // Collect processing building positions and whether they have inputs
    let workshops: Vec<(f64, f64, bool)> = world
        .query::<(&Position, &ProcessingBuilding)>()
        .iter()
        .map(|(p, b)| {
            let has_input = match b.recipe {
                // Threshold at 12: above Smithy cost (10w) so wood can accumulate to
                // afford a Smithy before Workshop drains it.  Above hut cost (6w) so
                // worker assignment doesn't fire when wood is too low for buildings.
                // Once assigned, progress pauses if wood dips below 12
                // (system_processing checks same threshold each tick).
                Recipe::WoodToPlanks => resources.wood >= 12,
                Recipe::StoneToMasonry => resources.stone >= 2,
                // Don't assign granary workers when food is near survival minimum
                Recipe::FoodToGrain => resources.food > 15,
                Recipe::GrainToBread => resources.grain >= 2 && resources.planks >= 1,
            };
            (p.x, p.y, has_input)
        })
        .collect();

    // Count villagers already assigned to each farm/workshop (within 1 tile of target)
    let mut farm_workers: Vec<usize> = vec![0; farms.len()];
    let mut workshop_workers: Vec<usize> = vec![0; workshops.len()];

    for behavior in world.query::<&Behavior>().iter() {
        match behavior.state {
            BehaviorState::Farming {
                target_x, target_y, ..
            } => {
                for (i, &(fx, fy, _, _)) in farms.iter().enumerate() {
                    if (fx - target_x).abs() < 1.0 && (fy - target_y).abs() < 1.0 {
                        farm_workers[i] += 1;
                        break;
                    }
                }
            }
            BehaviorState::Working {
                target_x, target_y, ..
            } => {
                for (i, &(wx, wy, _)) in workshops.iter().enumerate() {
                    if (wx - target_x).abs() < 1.0 && (wy - target_y).abs() < 1.0 {
                        workshop_workers[i] += 1;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    // Find idle/wandering villagers and assign them
    // Reserve at least 1/3 of villagers for free gathering (wood, stone, food)
    let total_villagers = world
        .query::<(&Creature, &Behavior)>()
        .iter()
        .filter(|(c, _)| c.species == Species::Villager)
        .count();
    // When wood is critically low AND food is safe, free up 2 extra villagers for resource
    // gathering. Stone deposit discovery keeps stone at 5-9, so the old farming break-off
    // condition (wood<5 && stone<5) almost never fires — this is the targeted fix.
    let wood_low = resources.wood < 8;
    let food_safe = resources.food > total_villagers as u32 * 2;
    let base_max = if wood_low && food_safe {
        (total_villagers * 2 / 3).saturating_sub(2).max(1)
    } else {
        (total_villagers * 2 / 3).max(1)
    };
    // Reserve extra slots for workshops that have input but no worker yet — without this,
    // farms fill every assignment slot and Workshop/Granary never gets a worker.
    let workshops_needing_worker = workshops
        .iter()
        .enumerate()
        .filter(|(i, w)| w.2 && workshop_workers[*i] == 0)
        .count();
    let max_assigned = base_max + workshops_needing_worker;
    let currently_assigned = world
        .query::<&Behavior>()
        .iter()
        .filter(|b| {
            matches!(
                b.state,
                BehaviorState::Farming { .. } | BehaviorState::Working { .. }
            )
        })
        .count();

    let mut assignments: Vec<(Entity, BehaviorState)> = Vec::new();

    for (e, pos, creature, behavior) in world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
    {
        if creature.species != Species::Villager {
            continue;
        }
        if creature.hunger > 0.5 {
            continue;
        }
        match behavior.state {
            BehaviorState::Idle { .. } | BehaviorState::Wander { .. } => {}
            _ => continue,
        }
        // Don't assign more than 2/3 of villagers to buildings — leave rest for gathering
        if currently_assigned + assignments.len() >= max_assigned {
            break;
        }

        // Priority 1: farms with pending food (need pickup + haul)
        let mut best_farm: Option<(usize, f64)> = None;
        for (i, &(fx, fy, _, pending)) in farms.iter().enumerate() {
            if pending > 0 && farm_workers[i] == 0 {
                let d = dist(pos.x, pos.y, fx, fy);
                if best_farm.is_none() || d < best_farm.unwrap().1 {
                    best_farm = Some((i, d));
                }
            }
        }
        if let Some((i, _)) = best_farm {
            let (fx, fy, _, _) = farms[i];
            farm_workers[i] += 1;
            assignments.push((
                e,
                BehaviorState::Farming {
                    target_x: fx,
                    target_y: fy,
                    lease: 200,
                },
            ));
            continue;
        }

        // Priority 2: workshops that have inputs and need a worker
        // (before farm tending so the reserved workshop slots don't get consumed by farms)
        let mut best_workshop: Option<(usize, f64)> = None;
        for (i, &(wx, wy, has_input)) in workshops.iter().enumerate() {
            if has_input && workshop_workers[i] == 0 {
                let d = dist(pos.x, pos.y, wx, wy);
                if best_workshop.is_none() || d < best_workshop.unwrap().1 {
                    best_workshop = Some((i, d));
                }
            }
        }
        if let Some((i, _)) = best_workshop {
            let (wx, wy, _) = workshops[i];
            workshop_workers[i] += 1;
            assignments.push((
                e,
                BehaviorState::Working {
                    target_x: wx,
                    target_y: wy,
                    lease: 200,
                },
            ));
            continue;
        }

        // Priority 3: farms that need tending (not harvest-ready, growth < 1.0)
        let mut best_tend: Option<(usize, f64)> = None;
        for (i, &(fx, fy, harvest_ready, _)) in farms.iter().enumerate() {
            if !harvest_ready && farm_workers[i] == 0 {
                let d = dist(pos.x, pos.y, fx, fy);
                if best_tend.is_none() || d < best_tend.unwrap().1 {
                    best_tend = Some((i, d));
                }
            }
        }
        if let Some((i, _)) = best_tend {
            let (fx, fy, _, _) = farms[i];
            farm_workers[i] += 1;
            assignments.push((
                e,
                BehaviorState::Farming {
                    target_x: fx,
                    target_y: fy,
                    lease: 200,
                },
            ));
        }
    }

    // Apply assignments
    for (e, new_state) in assignments {
        if let Ok(mut behavior) = world.get::<&mut Behavior>(e) {
            behavior.state = new_state;
        }
    }
}

/// Mark farms and workshops that have a villager worker nearby.
/// Also handles farm food pickup: if a villager is at a farm with pending_food,
/// pick it up and switch to hauling.
pub fn system_mark_workers(world: &mut World) -> u32 {
    // Collect villager positions and their behavior state
    let villager_positions: Vec<(Entity, f64, f64, BehaviorState)> = world
        .query::<(Entity, &Position, &Creature, &Behavior)>()
        .iter()
        .filter(|(_, _, c, _)| c.species == Species::Villager)
        .map(|(e, p, _, b)| (e, p.x, p.y, b.state))
        .collect();

    // Mark farms with workers and collect food pickups
    let mut food_pickups: Vec<(Entity, f64, f64, u32)> = Vec::new(); // (villager, stockpile_x, stockpile_y, amount)

    // Get stockpile positions for hauling
    let stockpiles: Vec<(f64, f64)> = world
        .query::<(&Position, &Stockpile)>()
        .iter()
        .map(|(p, _)| (p.x, p.y))
        .collect();

    let farm_entities: Vec<(Entity, f64, f64)> = world
        .query::<(Entity, &Position, &FarmPlot)>()
        .iter()
        .map(|(e, p, _)| (e, p.x, p.y))
        .collect();

    for &(farm_e, fx, fy) in &farm_entities {
        for &(ve, vx, vy, ref state) in &villager_positions {
            if matches!(state, BehaviorState::Farming { .. }) {
                let d = dist(vx, vy, fx, fy);
                if d < 2.5 {
                    // Worker is at this farm
                    if let Ok(mut farm) = world.get::<&mut FarmPlot>(farm_e) {
                        farm.worker_present = true;
                        if farm.pending_food > 0 {
                            let amount = farm.pending_food;
                            farm.pending_food = 0;
                            // Find nearest stockpile for hauling
                            let nearest = stockpiles
                                .iter()
                                .map(|&(sx, sy)| (sx, sy, dist(vx, vy, sx, sy)))
                                .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                            if let Some((sx, sy, _)) = nearest {
                                food_pickups.push((ve, sx, sy, amount));
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    // Apply food pickup — switch villager to Hauling
    let mut total_food_hauled = 0u32;
    for (ve, sx, sy, amount) in &food_pickups {
        if let Ok(mut behavior) = world.get::<&mut Behavior>(*ve) {
            behavior.state = BehaviorState::Hauling {
                target_x: *sx,
                target_y: *sy,
                resource_type: ResourceType::Food,
            };
        }
        total_food_hauled += amount;
    }

    // Mark workshops with workers
    let workshop_entities: Vec<(Entity, f64, f64)> = world
        .query::<(Entity, &Position, &ProcessingBuilding)>()
        .iter()
        .map(|(e, p, _)| (e, p.x, p.y))
        .collect();

    for &(ws_e, wx, wy) in &workshop_entities {
        for &(_, vx, vy, ref state) in &villager_positions {
            if matches!(state, BehaviorState::Working { .. }) {
                let d = dist(vx, vy, wx, wy);
                if d < 2.5 {
                    if let Ok(mut building) = world.get::<&mut ProcessingBuilding>(ws_e) {
                        building.worker_present = true;
                    }
                    break;
                }
            }
        }
    }

    total_food_hauled
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

/// Convert tile moisture to a growth-rate multiplier.
/// Floor of 0.4 (dry farms still work), caps at 1.0 when moisture >= 0.6.
fn moisture_ramp(moisture: f64) -> f64 {
    let t = (moisture / 0.6).clamp(0.0, 1.0);
    0.4 + 0.6 * t
}

/// Grow farm plots based on season and auto-harvest when ready.
/// Moisture from the simulation scales growth: river-adjacent farms grow faster.
pub fn system_farms(world: &mut World, season: Season, skill_mult: f64, moisture: &MoistureMap) {
    let base_rate = match season {
        Season::Spring => 0.002,
        Season::Summer => 0.003,
        Season::Autumn => 0.001,
        Season::Winter => 0.0,
    };

    // Pass 1: advance growth only if a worker is present
    for farm in world.query_mut::<&mut FarmPlot>() {
        if farm.harvest_ready {
            // Harvest: produce pending food for villager pickup
            if farm.worker_present {
                farm.growth = 0.0;
                farm.harvest_ready = false;
                farm.pending_food += 3;
            }
        } else if farm.worker_present {
            let moisture_val = moisture.get(farm.tile_x, farm.tile_y);
            let moisture_factor = moisture_ramp(moisture_val);
            let growth_rate = base_rate * skill_mult * moisture_factor;
            farm.growth += growth_rate;
            if farm.growth >= 1.0 {
                farm.growth = 1.0;
                farm.harvest_ready = true;
            }
        }
        // Reset worker flag each tick — villager AI sets it each tick they're working
        farm.worker_present = false;
    }

    // Pass 2: update sprite visuals based on growth stage
    for (farm, sprite) in world.query_mut::<(&FarmPlot, &mut Sprite)>() {
        if farm.pending_food > 0 {
            sprite.fg = Color(255, 200, 50); // bright gold — food waiting for pickup
            sprite.ch = '♣';
        } else if farm.harvest_ready {
            sprite.fg = Color(220, 200, 40); // harvest ready — gold
            sprite.ch = '♣';
        } else if farm.growth < 0.3 {
            sprite.fg = Color(120, 80, 30); // dirt
            sprite.ch = '·';
        } else if farm.growth < 0.7 {
            sprite.fg = Color(80, 160, 40); // growing
            sprite.ch = '♠';
        } else {
            sprite.fg = Color(60, 180, 40); // mature
            sprite.ch = '"';
        }
    }
}

/// Process resources in processing buildings (workshops, smithies).
/// Converts raw resources into refined ones based on recipe.
pub fn system_processing(world: &mut World, resources: &mut Resources, skill_mult: f64) {
    for (building, sprite) in world.query_mut::<(&mut ProcessingBuilding, &mut Sprite)>() {
        let has_input = match building.recipe {
            Recipe::WoodToPlanks => resources.wood >= 12,
            Recipe::StoneToMasonry => resources.stone >= 2,
            // Only convert food→grain when there's a comfortable surplus.
            // Without this guard, the granary drains food to 0 if bakery isn't built yet.
            Recipe::FoodToGrain => resources.food > 15,
            Recipe::GrainToBread => resources.grain >= 2 && resources.planks >= 1,
        };
        if has_input && building.worker_present {
            building.progress += 1;
            sprite.fg = Color(255, 200, 50); // bright yellow when active
        } else if !building.worker_present {
            sprite.fg = Color(80, 80, 80); // dark gray — no worker
        } else {
            sprite.fg = Color(100, 100, 100); // dim gray — no inputs
        }
        // Reset worker flag each tick
        building.worker_present = false;

        let speed_required = (building.required as f64 / skill_mult).max(1.0) as u32;
        if building.progress >= speed_required {
            building.progress = 0;
            match building.recipe {
                Recipe::WoodToPlanks => {
                    if resources.wood >= 2 {
                        resources.wood -= 2;
                        resources.planks += 1;
                    }
                }
                Recipe::StoneToMasonry => {
                    if resources.stone >= 2 {
                        resources.stone -= 2;
                        resources.masonry += 1;
                    }
                }
                Recipe::FoodToGrain => {
                    if resources.food >= 3 {
                        resources.food -= 3;
                        resources.grain += 2;
                    }
                }
                Recipe::GrainToBread => {
                    if resources.grain >= 2 && resources.planks >= 1 {
                        resources.grain -= 2;
                        resources.planks -= 1;
                        resources.bread += 3;
                    }
                }
            }
        }
    }
}

/// Breeding system: prey breed at dens in spring/summer, wolves breed when well-fed.
pub fn system_breeding(world: &mut World, season: Season, wolf_breed_boost: f64, year: u32) {
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
            let key = (
                creature.home_x.round() as i32,
                creature.home_y.round() as i32,
            );
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

    let wolf_cap = (4 + 2 * year) as usize;
    if wolf_count < wolf_cap {
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

        let wolf_threshold = (1000.0 / wolf_breed_boost) as u32;
        for (px, py) in wolf_candidates {
            if rng.random_range(0u32..wolf_threshold.max(1)) == 0 {
                spawn_predator(
                    world,
                    px + rng.random_range(-3i32..4) as f64,
                    py + rng.random_range(-3i32..4) as f64,
                );
            }
        }
    }
}

/// Coordinated wolf raid system: when 5+ wolves are within range of each other,
/// they attack as a pack toward the settlement center.
/// Returns true if a raid was launched this tick.
pub fn system_wolf_raids(
    world: &mut World,
    settlement_x: f64,
    settlement_y: f64,
    tick: u64,
    year: u32,
) -> bool {
    // Only check every 50 ticks to avoid constant scanning
    if !tick.is_multiple_of(50) {
        return false;
    }

    // Collect wolf positions
    let wolves: Vec<(Entity, f64, f64)> = world
        .query::<(Entity, &Position, &Creature)>()
        .iter()
        .filter(|(_, _, c)| c.species == Species::Predator)
        .map(|(e, p, _)| (e, p.x, p.y))
        .collect();

    let raid_threshold = 3u32.max(5u32.saturating_sub(year)) as usize;
    if wolves.len() < raid_threshold {
        return false;
    }

    // Find clusters of raid_threshold+ wolves within 15 tiles of each other
    let cluster_radius = 15.0;
    for &(_, wx, wy) in &wolves {
        let pack: Vec<Entity> = wolves
            .iter()
            .filter(|(_, x, y)| dist(wx, wy, *x, *y) < cluster_radius)
            .map(|(e, _, _)| *e)
            .collect();

        if pack.len() >= raid_threshold {
            // Launch raid: set all pack wolves to hunt toward settlement
            for wolf_e in pack {
                if let Ok(mut behavior) = world.get::<&mut Behavior>(wolf_e) {
                    behavior.state = BehaviorState::Hunting {
                        target_x: settlement_x,
                        target_y: settlement_y,
                    };
                }
            }
            return true;
        }
    }
    false
}

/// Per-villager memory observation and decay system.
/// Each tick, villagers observe their surroundings and record memories.
/// Confidence on all entries decays, and stale entries are evicted.
///
/// This is Phase 1: additive only. AI still reads global state for decisions.
/// Phase 2 (local_awareness #30) will switch AI to read from memory.
pub fn system_update_memories(
    world: &mut World,
    map: &TileMap,
    grid: &SpatialHashGrid,
    current_tick: u64,
) {
    // Snapshot entity positions for observation targets
    let food_positions: Vec<(f64, f64)> = grid
        .all_of_category(category::FOOD_SOURCE)
        .iter()
        .map(|e| (e.x, e.y))
        .collect();
    let stone_positions: Vec<(f64, f64)> = grid
        .all_of_category(category::STONE_DEPOSIT)
        .iter()
        .map(|e| (e.x, e.y))
        .collect();
    let build_site_positions: Vec<(f64, f64)> = grid
        .all_of_category(category::BUILD_SITE)
        .iter()
        .map(|e| (e.x, e.y))
        .collect();
    let predator_positions: Vec<(f64, f64)> = grid
        .all_of_category(category::PREDATOR)
        .iter()
        .map(|e| (e.x, e.y))
        .collect();
    let stockpile_positions: Vec<(f64, f64)> = grid
        .all_of_category(category::STOCKPILE)
        .iter()
        .map(|e| (e.x, e.y))
        .collect();

    // Collect villager entities first to avoid borrow issues
    let villager_entities: Vec<Entity> = world
        .query::<(Entity, &Creature, &VillagerMemory)>()
        .iter()
        .filter(|(_, c, _)| c.species == Species::Villager)
        .map(|(e, _, _)| e)
        .collect();

    for e in villager_entities {
        let Some((pos_x, pos_y, sight_range)) = world.get::<&Position>(e).ok().and_then(|p| {
            world
                .get::<&Creature>(e)
                .ok()
                .map(|c| (p.x, p.y, c.sight_range))
        }) else {
            continue;
        };

        let Ok(mut memory) = world.get::<&mut VillagerMemory>(e) else {
            continue;
        };

        // Record nearest stockpile location (pinned, does not need MemoryEntry)
        if memory.stockpile_loc.is_none() {
            let mut best_d = f64::MAX;
            for &(sx, sy) in &stockpile_positions {
                let d = ((pos_x - sx).powi(2) + (pos_y - sy).powi(2)).sqrt();
                if d < best_d {
                    best_d = d;
                    memory.stockpile_loc = Some((sx, sy));
                }
            }
        }

        // Observe terrain within sight range (sample 8 directions at 3 distances)
        let sr_sq = sight_range * sight_range;
        for &sample_dist in &[3.0, 6.0, 12.0] {
            for &(dx, dy) in &EIGHT_DIRS {
                let sx = pos_x + dx * sample_dist;
                let sy = pos_y + dy * sample_dist;
                let dsq = (pos_x - sx).powi(2) + (pos_y - sy).powi(2);
                if dsq > sr_sq {
                    continue;
                }
                let tx = sx.round() as i64;
                let ty = sy.round() as i64;
                if tx < 0 || ty < 0 {
                    continue;
                }
                match map.get(tx as usize, ty as usize) {
                    Some(Terrain::Forest) => {
                        memory.upsert(MemoryKind::WoodSource, sx, sy, current_tick);
                    }
                    Some(Terrain::Mountain) => {
                        memory.upsert(MemoryKind::StoneDeposit, sx, sy, current_tick);
                    }
                    _ => {}
                }
            }
        }

        // Observe entities within sight range
        for &(fx, fy) in &food_positions {
            let dsq = (pos_x - fx).powi(2) + (pos_y - fy).powi(2);
            if dsq < sr_sq {
                memory.upsert(MemoryKind::FoodSource, fx, fy, current_tick);
            }
        }
        for &(sx, sy) in &stone_positions {
            let dsq = (pos_x - sx).powi(2) + (pos_y - sy).powi(2);
            if dsq < sr_sq {
                memory.upsert(MemoryKind::StoneDeposit, sx, sy, current_tick);
            }
        }
        for &(bx, by) in &build_site_positions {
            let dsq = (pos_x - bx).powi(2) + (pos_y - by).powi(2);
            if dsq < sr_sq {
                memory.upsert(MemoryKind::BuildSite, bx, by, current_tick);
            }
        }
        for &(px, py) in &predator_positions {
            let dsq = (pos_x - px).powi(2) + (pos_y - py).powi(2);
            if dsq < sr_sq {
                memory.upsert(MemoryKind::DangerZone, px, py, current_tick);
            }
        }

        // Decay all entries
        memory.decay_tick();
    }
}
