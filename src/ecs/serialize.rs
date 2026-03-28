use hecs::World;
use serde::{Serialize, Deserialize};

use super::components::*;

// --- Serialization ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializedEntity {
    Villager { pos: Position, vel: Velocity, sprite: Sprite, behavior: Behavior, creature: Creature },
    Prey { pos: Position, vel: Velocity, sprite: Sprite, behavior: Behavior, creature: Creature },
    Predator { pos: Position, vel: Velocity, sprite: Sprite, behavior: Behavior, creature: Creature },
    FoodSource { pos: Position, sprite: Sprite, yield_info: Option<ResourceYield> },
    StoneDeposit { pos: Position, sprite: Sprite, yield_info: Option<ResourceYield> },
    Den { pos: Position, sprite: Sprite },
    StockpileEntity { pos: Position, sprite: Sprite },
    BuildSiteEntity { pos: Position, sprite: Sprite, site: BuildSite },
    FarmPlotEntity { pos: Position, sprite: Sprite, farm: FarmPlot },
    ProcessingBuildingEntity { pos: Position, sprite: Sprite, building: ProcessingBuilding },
    GarrisonEntity { pos: Position, sprite: Sprite, garrison: GarrisonBuilding },
    HutEntity { pos: Position, sprite: Sprite, hut: HutBuilding },
}

pub fn serialize_world(world: &World) -> Vec<SerializedEntity> {
    let mut entities = Vec::new();

    for (pos, vel, sprite, behavior, creature) in
        world.query::<(&Position, &Velocity, &Sprite, &Behavior, &Creature)>().iter()
    {
        let se = match creature.species {
            Species::Villager => SerializedEntity::Villager {
                pos: *pos, vel: *vel, sprite: *sprite, behavior: *behavior, creature: *creature,
            },
            Species::Prey => SerializedEntity::Prey {
                pos: *pos, vel: *vel, sprite: *sprite, behavior: *behavior, creature: *creature,
            },
            Species::Predator => SerializedEntity::Predator {
                pos: *pos, vel: *vel, sprite: *sprite, behavior: *behavior, creature: *creature,
            },
        };
        entities.push(se);
    }

    for (pos, sprite, _, ry) in world.query::<(&Position, &Sprite, &FoodSource, Option<&ResourceYield>)>().iter() {
        entities.push(SerializedEntity::FoodSource { pos: *pos, sprite: *sprite, yield_info: ry.copied() });
    }
    for (pos, sprite, _, ry) in world.query::<(&Position, &Sprite, &StoneDeposit, Option<&ResourceYield>)>().iter() {
        entities.push(SerializedEntity::StoneDeposit { pos: *pos, sprite: *sprite, yield_info: ry.copied() });
    }
    for (pos, sprite, _) in world.query::<(&Position, &Sprite, &Den)>().iter() {
        entities.push(SerializedEntity::Den { pos: *pos, sprite: *sprite });
    }
    for (pos, sprite, _) in world.query::<(&Position, &Sprite, &Stockpile)>().iter() {
        entities.push(SerializedEntity::StockpileEntity { pos: *pos, sprite: *sprite });
    }
    for (pos, sprite, site) in world.query::<(&Position, &Sprite, &BuildSite)>().iter() {
        entities.push(SerializedEntity::BuildSiteEntity { pos: *pos, sprite: *sprite, site: *site });
    }
    for (pos, sprite, farm) in world.query::<(&Position, &Sprite, &FarmPlot)>().iter() {
        entities.push(SerializedEntity::FarmPlotEntity { pos: *pos, sprite: *sprite, farm: *farm });
    }
    for (pos, sprite, building) in world.query::<(&Position, &Sprite, &ProcessingBuilding)>().iter() {
        entities.push(SerializedEntity::ProcessingBuildingEntity { pos: *pos, sprite: *sprite, building: *building });
    }
    for (pos, sprite, garrison) in world.query::<(&Position, &Sprite, &GarrisonBuilding)>().iter() {
        entities.push(SerializedEntity::GarrisonEntity { pos: *pos, sprite: *sprite, garrison: *garrison });
    }
    for (pos, sprite, hut) in world.query::<(&Position, &Sprite, &HutBuilding)>().iter() {
        entities.push(SerializedEntity::HutEntity { pos: *pos, sprite: *sprite, hut: *hut });
    }

    entities
}

pub fn deserialize_world(entities: &[SerializedEntity]) -> World {
    let mut world = World::new();
    for entity in entities {
        match entity {
            SerializedEntity::Villager { pos, vel, sprite, behavior, creature } => {
                world.spawn((*pos, *vel, *sprite, *behavior, *creature));
            }
            SerializedEntity::Prey { pos, vel, sprite, behavior, creature } => {
                world.spawn((*pos, *vel, *sprite, *behavior, *creature));
            }
            SerializedEntity::Predator { pos, vel, sprite, behavior, creature } => {
                world.spawn((*pos, *vel, *sprite, *behavior, *creature));
            }
            SerializedEntity::FoodSource { pos, sprite, yield_info } => {
                let ry = yield_info.unwrap_or(ResourceYield { remaining: 6, max: 6 });
                world.spawn((*pos, *sprite, FoodSource, ry));
            }
            SerializedEntity::StoneDeposit { pos, sprite, yield_info } => {
                let ry = yield_info.unwrap_or(ResourceYield { remaining: 5, max: 5 });
                world.spawn((*pos, *sprite, StoneDeposit, ry));
            }
            SerializedEntity::Den { pos, sprite } => {
                world.spawn((*pos, *sprite, Den));
            }
            SerializedEntity::StockpileEntity { pos, sprite } => {
                world.spawn((*pos, *sprite, Stockpile));
            }
            SerializedEntity::BuildSiteEntity { pos, sprite, site } => {
                world.spawn((*pos, *sprite, *site));
            }
            SerializedEntity::FarmPlotEntity { pos, sprite, farm } => {
                world.spawn((*pos, *sprite, *farm));
            }
            SerializedEntity::ProcessingBuildingEntity { pos, sprite, building } => {
                world.spawn((*pos, *sprite, *building));
            }
            SerializedEntity::GarrisonEntity { pos, sprite, garrison } => {
                world.spawn((*pos, *sprite, *garrison));
            }
            SerializedEntity::HutEntity { pos, sprite, hut } => {
                world.spawn((*pos, *sprite, *hut));
            }
        }
    }
    world
}
