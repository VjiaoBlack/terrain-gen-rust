use serde::{Deserialize, Serialize};

use crate::renderer::Color;
use crate::tilemap::Terrain;

// --- AI Result ---

/// Skill-derived multipliers passed into AI systems.
pub struct SkillMults {
    pub gather_wood_speed: f64, // multiplier on gathering timer (lower = faster)
    pub gather_stone_speed: f64,
    pub build_speed: u32, // extra progress per tick
}

impl Default for SkillMults {
    fn default() -> Self {
        Self {
            gather_wood_speed: 1.0,
            gather_stone_speed: 1.0,
            build_speed: 0,
        }
    }
}

/// Result of running the AI system for one tick.
pub struct AiResult {
    pub deposited: Vec<ResourceType>,
    pub food_consumed: u32,
    pub grain_consumed: u32,
    pub bread_consumed: u32,
    pub farming_ticks: u32,
    pub mining_ticks: u32,
    pub woodcutting_ticks: u32,
    pub building_ticks: u32,
}

// --- Components ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Velocity {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sprite {
    pub ch: char,
    pub fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Species {
    Prey,
    Predator,
    Villager,
}

// Resource types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResourceType {
    Food,
    Wood,
    Stone,
    Planks,
    Masonry,
    Grain,
}

/// Marker for stockpile location (where villagers deposit resources).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Stockpile;

/// Resource carried by a villager or stored at stockpile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CarriedResource {
    pub resource_type: ResourceType,
    pub amount: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SeekReason {
    Food,
    Stockpile,
    BuildSite,
    Wood,
    Stone,
    Hut,
    ExitBuilding,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BehaviorState {
    /// Wander randomly. Timer counts down to next direction change.
    Wander { timer: u32 },
    /// Move toward a target position with a reason for debugging.
    Seek {
        target_x: f64,
        target_y: f64,
        reason: SeekReason,
    },
    /// Stand still. Timer counts down before switching to Wander.
    Idle { timer: u32 },
    /// Prey: eating at a food source.
    Eating { timer: u32 },
    /// Prey/villager: fleeing home because predator is nearby. Timer prevents getting stuck.
    FleeHome { timer: u32 },
    /// Prey: safe at home, resting until hungry.
    AtHome { timer: u32 },
    /// Predator: chasing a prey it spotted.
    Hunting { target_x: f64, target_y: f64 },
    /// Prey: captured by a predator, frozen in place until consumed.
    Captured,
    /// Villager: gathering a resource at a location.
    Gathering {
        timer: u32,
        resource_type: ResourceType,
    },
    /// Villager: hauling gathered resource back to stockpile.
    Hauling {
        target_x: f64,
        target_y: f64,
        resource_type: ResourceType,
    },
    /// Villager: sleeping at night.
    Sleeping { timer: u32 },
    /// Villager: building at a build site.
    Building {
        target_x: f64,
        target_y: f64,
        timer: u32,
    },
    /// Villager: tending a farm (standing at farm, advancing growth).
    Farming { target_x: f64, target_y: f64 },
    /// Villager: operating a workshop/smithy (standing at building, advancing processing).
    Working { target_x: f64, target_y: f64 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Behavior {
    pub state: BehaviorState,
    pub speed: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Creature {
    pub species: Species,
    pub hunger: f64, // 0.0 = full, 1.0 = starving
    pub home_x: f64,
    pub home_y: f64,
    pub sight_range: f64, // how far this creature can see
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BuildingType {
    Hut,
    Wall,
    Farm,
    Stockpile,
    Workshop,
    Smithy,
    Garrison,
    Road,
    Granary,
    Bakery,
    TownHall,
}

/// Tile layout pattern for a building footprint.
#[derive(Debug, Clone, Copy)]
pub enum TileLayout {
    /// Single tile of the given terrain type (Wall, Road)
    Single(Terrain),
    /// Filled rectangle of BuildingFloor (Farm, Stockpile)
    FilledFloor,
    /// 3x3 walls with center floor and a door on the south side (Hut)
    WallsDoorSouth,
    /// 3x3 walls with center floor and a door on the north side (Workshop, Smithy, Granary, Bakery)
    WallsDoorNorth,
    /// 3x3 walls with center floor, no door (Garrison)
    WallsNoDoor,
}

/// Static definition of a building type's properties.
pub struct BuildingDef {
    pub name: &'static str,
    pub cost: Resources,
    pub build_time: u32,
    pub size: (i32, i32),
    pub layout: TileLayout,
}

impl BuildingType {
    /// Get the static definition for this building type.
    pub fn def(&self) -> BuildingDef {
        match self {
            BuildingType::Hut => BuildingDef {
                name: "Hut",
                cost: Resources {
                    wood: 10,
                    stone: 4,
                    ..DEF_RES
                },
                build_time: 180,
                size: (3, 3),
                layout: TileLayout::WallsDoorSouth,
            },
            BuildingType::Wall => BuildingDef {
                name: "Wall",
                cost: Resources {
                    wood: 2,
                    stone: 2,
                    ..DEF_RES
                },
                build_time: 45,
                size: (1, 1),
                layout: TileLayout::Single(Terrain::BuildingWall),
            },
            BuildingType::Farm => BuildingDef {
                name: "Farm",
                cost: Resources {
                    wood: 5,
                    stone: 1,
                    ..DEF_RES
                },
                build_time: 120,
                size: (3, 3),
                layout: TileLayout::FilledFloor,
            },
            BuildingType::Stockpile => BuildingDef {
                name: "Stockpile",
                cost: Resources { wood: 4, ..DEF_RES },
                build_time: 60,
                size: (2, 2),
                layout: TileLayout::FilledFloor,
            },
            BuildingType::Workshop => BuildingDef {
                name: "Workshop",
                cost: Resources {
                    wood: 8,
                    stone: 3,
                    ..DEF_RES
                },
                build_time: 220,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::Smithy => BuildingDef {
                name: "Smithy",
                cost: Resources {
                    wood: 10,
                    stone: 15,
                    ..DEF_RES
                },
                build_time: 270,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::Garrison => BuildingDef {
                name: "Garrison",
                cost: Resources {
                    planks: 10,
                    masonry: 10,
                    ..DEF_RES
                },
                build_time: 180,
                size: (3, 3),
                layout: TileLayout::WallsNoDoor,
            },
            BuildingType::Road => BuildingDef {
                name: "Road",
                cost: Resources {
                    stone: 2,
                    ..DEF_RES
                },
                build_time: 30,
                size: (1, 1),
                layout: TileLayout::Single(Terrain::Road),
            },
            BuildingType::Granary => BuildingDef {
                name: "Granary",
                cost: Resources {
                    wood: 6,
                    stone: 4,
                    ..DEF_RES
                },
                build_time: 240,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::Bakery => BuildingDef {
                name: "Bakery",
                cost: Resources {
                    wood: 8,
                    stone: 6,
                    planks: 5,
                    ..DEF_RES
                },
                build_time: 210,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::TownHall => BuildingDef {
                name: "Town Hall",
                cost: Resources {
                    wood: 20,
                    stone: 30,
                    masonry: 80,
                    ..DEF_RES
                },
                build_time: 400,
                size: (3, 3),
                layout: TileLayout::WallsNoDoor,
            },
        }
    }

    pub fn cost(&self) -> Resources {
        self.def().cost
    }
    pub fn build_time(&self) -> u32 {
        self.def().build_time
    }
    pub fn size(&self) -> (i32, i32) {
        self.def().size
    }
    pub fn name(&self) -> &'static str {
        self.def().name
    }

    pub fn tiles(&self) -> Vec<(i32, i32, Terrain)> {
        let d = self.def();
        let (w, h) = d.size;
        match d.layout {
            TileLayout::Single(terrain) => vec![(0, 0, terrain)],
            TileLayout::FilledFloor => {
                let mut tiles = Vec::new();
                for dy in 0..h {
                    for dx in 0..w {
                        tiles.push((dx, dy, Terrain::BuildingFloor));
                    }
                }
                tiles
            }
            TileLayout::WallsDoorSouth => {
                let mut tiles = Vec::new();
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingWall));
                    tiles.push((dx, 2, Terrain::BuildingWall));
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor));
                tiles.push((1, 2, Terrain::BuildingFloor)); // door south
                tiles
            }
            TileLayout::WallsDoorNorth => {
                let mut tiles = Vec::new();
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingWall));
                    tiles.push((dx, 2, Terrain::BuildingWall));
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor));
                tiles.push((1, 0, Terrain::BuildingFloor)); // door north
                tiles
            }
            TileLayout::WallsNoDoor => {
                let mut tiles = Vec::new();
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingWall));
                    tiles.push((dx, 2, Terrain::BuildingWall));
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor));
                tiles
            }
        }
    }

    pub fn all() -> &'static [BuildingType] {
        &[
            BuildingType::Hut,
            BuildingType::Wall,
            BuildingType::Farm,
            BuildingType::Stockpile,
            BuildingType::Workshop,
            BuildingType::Smithy,
            BuildingType::Garrison,
            BuildingType::Road,
            BuildingType::Granary,
            BuildingType::Bakery,
            BuildingType::TownHall,
        ]
    }
}

/// Zero-valued Resources constant for struct update syntax in const-like contexts.
const DEF_RES: Resources = Resources {
    food: 0,
    wood: 0,
    stone: 0,
    planks: 0,
    masonry: 0,
    grain: 0,
    bread: 0,
};

/// A build site entity — placed by the player, worked on by villagers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BuildSite {
    pub building_type: BuildingType,
    pub progress: u32,
    pub required: u32,
    pub assigned: bool,
}

/// Marker for a completed farm plot — grows crops and produces food.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FarmPlot {
    pub growth: f64, // 0.0 to 1.0
    pub harvest_ready: bool,
    #[serde(default)]
    pub worker_present: bool, // must have villager tending for growth
    #[serde(default)]
    pub pending_food: u32, // harvested food waiting for pickup
}

/// Marker component for berry bushes (food source for prey).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FoodSource;

/// Marker component for completed garrison buildings — provides defense bonus.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GarrisonBuilding {
    pub defense_bonus: f64,
}

/// Marker component for Town Hall — provides housing bonus and extends settlement influence.
/// The Town Hall is a late-game prestige building that sinks accumulated masonry/stone and
/// allows the settlement to house more villagers without building more huts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TownHallBuilding {
    pub housing_bonus: u32,
}

/// Marker component for completed huts — provides shelter for villagers at night.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HutBuilding {
    pub capacity: u32,
    pub occupants: u32,
}

/// Tracks remaining harvests for a resource entity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceYield {
    pub remaining: u32,
    pub max: u32,
}

/// Marker component for stone deposits (mineable by villagers).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StoneDeposit;

/// Marker component for dens (safe home for prey).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Den;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Recipe {
    WoodToPlanks,   // 2 Wood -> 1 Planks
    StoneToMasonry, // 2 Stone -> 1 Masonry
    FoodToGrain,    // 3 Food -> 2 Grain
    GrainToBread,   // 2 Grain + 1 Wood -> 3 Bread (highest food value)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProcessingBuilding {
    pub recipe: Recipe,
    pub progress: u32,
    pub required: u32, // ticks per processing cycle
    #[serde(default)]
    pub worker_present: bool, // must have villager operating for progress
}

// --- Resources ---

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resources {
    pub food: u32,
    pub wood: u32,
    pub stone: u32,
    pub planks: u32,
    pub masonry: u32,
    pub grain: u32,
    #[serde(default)]
    pub bread: u32,
}

impl Resources {
    pub fn can_afford(&self, cost: &Resources) -> bool {
        self.food >= cost.food
            && self.wood >= cost.wood
            && self.stone >= cost.stone
            && self.planks >= cost.planks
            && self.masonry >= cost.masonry
            && self.grain >= cost.grain
            && self.bread >= cost.bread
    }

    pub fn deduct(&mut self, cost: &Resources) {
        self.food -= cost.food;
        self.wood -= cost.wood;
        self.stone -= cost.stone;
        self.planks -= cost.planks;
        self.masonry -= cost.masonry;
        self.grain -= cost.grain;
        self.bread -= cost.bread;
    }
}
