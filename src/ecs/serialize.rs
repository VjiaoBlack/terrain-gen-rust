use hecs::World;
use serde::{Deserialize, Serialize};

use super::components::*;

// --- Serialization ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializedEntity {
    Villager {
        pos: Position,
        vel: Velocity,
        sprite: Sprite,
        behavior: Behavior,
        creature: Creature,
        #[serde(default)]
        tick_schedule: Option<TickSchedule>,
        #[serde(default)]
        memory: Option<VillagerMemory>,
    },
    Prey {
        pos: Position,
        vel: Velocity,
        sprite: Sprite,
        behavior: Behavior,
        creature: Creature,
    },
    Predator {
        pos: Position,
        vel: Velocity,
        sprite: Sprite,
        behavior: Behavior,
        creature: Creature,
    },
    FoodSource {
        pos: Position,
        sprite: Sprite,
        yield_info: Option<ResourceYield>,
    },
    StoneDeposit {
        pos: Position,
        sprite: Sprite,
        yield_info: Option<ResourceYield>,
    },
    Den {
        pos: Position,
        sprite: Sprite,
    },
    StockpileEntity {
        pos: Position,
        sprite: Sprite,
        #[serde(default)]
        board: Option<BulletinBoard>,
    },
    BuildSiteEntity {
        pos: Position,
        sprite: Sprite,
        site: BuildSite,
    },
    FarmPlotEntity {
        pos: Position,
        sprite: Sprite,
        farm: FarmPlot,
    },
    ProcessingBuildingEntity {
        pos: Position,
        sprite: Sprite,
        building: ProcessingBuilding,
    },
    GarrisonEntity {
        pos: Position,
        sprite: Sprite,
        garrison: GarrisonBuilding,
        #[serde(default)]
        board: Option<GarrisonBoard>,
    },
    TownHallEntity {
        pos: Position,
        sprite: Sprite,
        town_hall: TownHallBuilding,
    },
    HutEntity {
        pos: Position,
        sprite: Sprite,
        hut: HutBuilding,
    },
}

pub fn serialize_world(world: &World) -> Vec<SerializedEntity> {
    let mut entities = Vec::new();

    for (pos, vel, sprite, behavior, creature, schedule, memory) in world
        .query::<(
            &Position,
            &Velocity,
            &Sprite,
            &Behavior,
            &Creature,
            Option<&TickSchedule>,
            Option<&VillagerMemory>,
        )>()
        .iter()
    {
        let se = match creature.species {
            Species::Villager => SerializedEntity::Villager {
                pos: *pos,
                vel: *vel,
                sprite: *sprite,
                behavior: *behavior,
                creature: *creature,
                tick_schedule: schedule.copied(),
                memory: memory.cloned(),
            },
            Species::Prey => SerializedEntity::Prey {
                pos: *pos,
                vel: *vel,
                sprite: *sprite,
                behavior: *behavior,
                creature: *creature,
            },
            Species::Predator => SerializedEntity::Predator {
                pos: *pos,
                vel: *vel,
                sprite: *sprite,
                behavior: *behavior,
                creature: *creature,
            },
        };
        entities.push(se);
    }

    for (pos, sprite, _, ry) in world
        .query::<(&Position, &Sprite, &FoodSource, Option<&ResourceYield>)>()
        .iter()
    {
        entities.push(SerializedEntity::FoodSource {
            pos: *pos,
            sprite: *sprite,
            yield_info: ry.copied(),
        });
    }
    for (pos, sprite, _, ry) in world
        .query::<(&Position, &Sprite, &StoneDeposit, Option<&ResourceYield>)>()
        .iter()
    {
        entities.push(SerializedEntity::StoneDeposit {
            pos: *pos,
            sprite: *sprite,
            yield_info: ry.copied(),
        });
    }
    for (pos, sprite, _) in world.query::<(&Position, &Sprite, &Den)>().iter() {
        entities.push(SerializedEntity::Den {
            pos: *pos,
            sprite: *sprite,
        });
    }
    for (pos, sprite, _, board) in world
        .query::<(&Position, &Sprite, &Stockpile, Option<&BulletinBoard>)>()
        .iter()
    {
        entities.push(SerializedEntity::StockpileEntity {
            pos: *pos,
            sprite: *sprite,
            board: board.cloned(),
        });
    }
    for (pos, sprite, site) in world.query::<(&Position, &Sprite, &BuildSite)>().iter() {
        entities.push(SerializedEntity::BuildSiteEntity {
            pos: *pos,
            sprite: *sprite,
            site: *site,
        });
    }
    for (pos, sprite, farm) in world.query::<(&Position, &Sprite, &FarmPlot)>().iter() {
        entities.push(SerializedEntity::FarmPlotEntity {
            pos: *pos,
            sprite: *sprite,
            farm: *farm,
        });
    }
    for (pos, sprite, building) in world
        .query::<(&Position, &Sprite, &ProcessingBuilding)>()
        .iter()
    {
        entities.push(SerializedEntity::ProcessingBuildingEntity {
            pos: *pos,
            sprite: *sprite,
            building: *building,
        });
    }
    for (pos, sprite, garrison, board) in world
        .query::<(
            &Position,
            &Sprite,
            &GarrisonBuilding,
            Option<&GarrisonBoard>,
        )>()
        .iter()
    {
        entities.push(SerializedEntity::GarrisonEntity {
            pos: *pos,
            sprite: *sprite,
            garrison: *garrison,
            board: board.cloned(),
        });
    }
    for (pos, sprite, town_hall) in world
        .query::<(&Position, &Sprite, &TownHallBuilding)>()
        .iter()
    {
        entities.push(SerializedEntity::TownHallEntity {
            pos: *pos,
            sprite: *sprite,
            town_hall: *town_hall,
        });
    }
    for (pos, sprite, hut) in world.query::<(&Position, &Sprite, &HutBuilding)>().iter() {
        entities.push(SerializedEntity::HutEntity {
            pos: *pos,
            sprite: *sprite,
            hut: *hut,
        });
    }

    entities
}

pub fn deserialize_world(entities: &[SerializedEntity]) -> World {
    let mut world = World::new();
    for entity in entities {
        match entity {
            SerializedEntity::Villager {
                pos,
                vel,
                sprite,
                behavior,
                creature,
                tick_schedule,
                memory,
            } => {
                // Reset next_ai_tick to 0 on load so entities re-evaluate immediately
                let schedule = TickSchedule {
                    next_ai_tick: 0,
                    interval: tick_schedule.map(|s| s.interval).unwrap_or(1),
                };
                let mem = memory.clone().unwrap_or_else(|| {
                    let mut m = VillagerMemory::new();
                    m.home = Some((pos.x, pos.y));
                    m
                });
                world.spawn((
                    *pos,
                    *vel,
                    *sprite,
                    *behavior,
                    *creature,
                    schedule,
                    PathCache::default(),
                    mem,
                ));
            }
            SerializedEntity::Prey {
                pos,
                vel,
                sprite,
                behavior,
                creature,
            } => {
                world.spawn((*pos, *vel, *sprite, *behavior, *creature));
            }
            SerializedEntity::Predator {
                pos,
                vel,
                sprite,
                behavior,
                creature,
            } => {
                world.spawn((*pos, *vel, *sprite, *behavior, *creature));
            }
            SerializedEntity::FoodSource {
                pos,
                sprite,
                yield_info,
            } => {
                let ry = yield_info.unwrap_or(ResourceYield {
                    remaining: 6,
                    max: 6,
                });
                world.spawn((*pos, *sprite, FoodSource, ry));
            }
            SerializedEntity::StoneDeposit {
                pos,
                sprite,
                yield_info,
            } => {
                let ry = yield_info.unwrap_or(ResourceYield {
                    remaining: 5,
                    max: 5,
                });
                world.spawn((*pos, *sprite, StoneDeposit, ry));
            }
            SerializedEntity::Den { pos, sprite } => {
                world.spawn((*pos, *sprite, Den));
            }
            SerializedEntity::StockpileEntity { pos, sprite, board } => {
                let bb = board.clone().unwrap_or_default();
                world.spawn((*pos, *sprite, Stockpile, bb));
            }
            SerializedEntity::BuildSiteEntity { pos, sprite, site } => {
                world.spawn((*pos, *sprite, *site));
            }
            SerializedEntity::FarmPlotEntity { pos, sprite, farm } => {
                world.spawn((*pos, *sprite, *farm));
            }
            SerializedEntity::ProcessingBuildingEntity {
                pos,
                sprite,
                building,
            } => {
                world.spawn((*pos, *sprite, *building));
            }
            SerializedEntity::GarrisonEntity {
                pos,
                sprite,
                garrison,
                board,
            } => {
                let gb = board.clone().unwrap_or_default();
                world.spawn((*pos, *sprite, *garrison, gb));
            }
            SerializedEntity::TownHallEntity {
                pos,
                sprite,
                town_hall,
            } => {
                world.spawn((*pos, *sprite, *town_hall));
            }
            SerializedEntity::HutEntity { pos, sprite, hut } => {
                world.spawn((*pos, *sprite, *hut));
            }
        }
    }
    world
}
