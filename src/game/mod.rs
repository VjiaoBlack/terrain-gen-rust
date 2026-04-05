mod build;
pub mod chokepoint;
pub mod dirty;
mod events;
mod fire;
mod input;
mod particles;
mod render;
mod save;
mod water_cycle;

#[cfg(test)]
mod tests;

use anyhow::Result;
use hecs::World;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::pathfinding::NavGraph;

#[allow(unused_imports)] // Some imports used only in #[cfg(test)] blocks
use crate::ecs::{
    self, Behavior, BehaviorState, BuildSite, BuildingType, Creature, Den, FarmPlot, FoodSource,
    GarrisonBuilding, HutBuilding, Position, ProcessingBuilding, Recipe, ResourceType, Resources,
    SeekReason, SerializedEntity, SkillMults, Species, Sprite, Stockpile, StoneDeposit,
    TownHallBuilding,
};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::simulation::{
    DayNightCycle, ExplorationMap, InfluenceMap, MoistureMap, ScentMap, Season, SimConfig,
    SoilFertilityMap, ThreatMap, TrafficMap, VegetationMap, WaterMap, WindField,
};
use crate::terrain_gen::TerrainGenConfig;
use crate::tilemap::{Camera, Terrain, TileMap};

pub const MAX_PARTICLES: usize = 200;

pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub ch: char,
    pub fg: Color,
    pub life: u32,
    pub max_life: u32,
    pub dx: f64,
    pub dy: f64,
    pub emissive: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameSnapshot {
    pub tick: u64,
    pub width: u16,
    pub height: u16,
    pub text: String,
    pub cells: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CellChange {
    pub x: u16,
    pub y: u16,
    pub old: Cell,
    pub new: Cell,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameDiff {
    pub from_tick: u64,
    pub to_tick: u64,
    pub changes: Vec<CellChange>,
}

impl FrameSnapshot {
    pub fn diff(&self, next: &FrameSnapshot) -> FrameDiff {
        let mut changes = Vec::new();
        for (y, (old_row, new_row)) in self.cells.iter().zip(next.cells.iter()).enumerate() {
            for (x, (old_cell, new_cell)) in old_row.iter().zip(new_row.iter()).enumerate() {
                if old_cell != new_cell {
                    changes.push(CellChange {
                        x: x as u16,
                        y: y as u16,
                        old: *old_cell,
                        new: *new_cell,
                    });
                }
            }
        }
        FrameDiff {
            from_tick: self.tick,
            to_tick: next.tick,
            changes,
        }
    }
}

pub use input::GameInput;

/// Width of the left-side UI panel in screen columns.
pub const PANEL_WIDTH: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Atmospheric view: lighting, seasons, water shimmer, weather effects.
    Normal,
    /// Symbolic map: flat colors, semantic glyphs, no lighting. Gameplay readability mode.
    Map,
    /// Painterly landscape: texture chars, hand-picked palettes, full lighting.
    /// Color carries all meaning; characters are invisible surface noise.
    Landscape,
    /// Developer debug view: uppercase terrain letters, raw data.
    Debug,
}

impl RenderMode {
    /// Cycle to the next render mode: Normal -> Map -> Landscape -> Debug -> Normal.
    pub fn next(self) -> Self {
        match self {
            RenderMode::Normal => RenderMode::Map,
            RenderMode::Map => RenderMode::Landscape,
            RenderMode::Landscape => RenderMode::Debug,
            RenderMode::Debug => RenderMode::Normal,
        }
    }

    /// Short status bar label.
    pub fn label(self) -> &'static str {
        match self {
            RenderMode::Normal => "Normal",
            RenderMode::Map => "Map",
            RenderMode::Landscape => "Landscape",
            RenderMode::Debug => "Debug",
        }
    }

    /// Descriptive subtitle shown in notifications when the view changes.
    pub fn description(self) -> &'static str {
        match self {
            RenderMode::Normal => "atmospheric, full lighting",
            RenderMode::Map => "symbolic, no lighting",
            RenderMode::Landscape => "painterly",
            RenderMode::Debug => "raw data",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayMode {
    None,
    Tasks,     // Color-code villagers by current activity
    Resources, // Show resource locations with color markers
    Threats,   // Show wolf positions and danger zones
    Traffic,   // Show foot traffic heatmap
    Territory, // Show settlement influence/culture borders
    Wind,      // Show wind direction arrows and speed
    WindFlow,  // Show wind as animated particles (no arrows)
    Height,    // Show raw height values as grayscale
}

/// Default raid strength for backward-compatible deserialization of old saves.
fn default_raid_strength() -> f64 {
    9.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    Drought {
        ticks_remaining: u64,
    },
    BountifulHarvest {
        ticks_remaining: u64,
    },
    Migration {
        count: u32,
    },
    WolfSurge {
        ticks_remaining: u64,
    },
    Plague {
        ticks_remaining: u64,
        kills_remaining: u32,
    },
    Blizzard {
        ticks_remaining: u64,
    },
    BanditRaid {
        stolen: bool,
        /// Raider party strength (count * 3.0). Higher = harder to repel.
        #[serde(default = "default_raid_strength")]
        strength: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Milestone {
    // Explore phase
    FirstWoodGathered,
    FirstStoneFound,
    FirstFarm,
    FirstHut,
    #[serde(alias = "FirstWinter")]
    FirstWinterSurvived,
    // Expand phase
    #[serde(alias = "TenVillagers")]
    PopulationTen,
    FirstWorkshop,
    FirstSmith,
    FirstRoad,
    FiveBuildings,
    // Exploit phase
    #[serde(alias = "TwentyVillagers")]
    PopulationTwentyFive,
    FirstGranary,
    FirstBakery,
    FirstPlank,
    HundredFood,
    // Endure phase
    #[serde(alias = "FirstGarrison")]
    FirstGarrison,
    RaidSurvived,
    PopulationFifty,
}

/// Banner displayed when a milestone is achieved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneBanner {
    pub message: String,
    pub ticks_remaining: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DifficultyState {
    pub threat_level: f64,
    pub milestones: Vec<Milestone>,
}

/// Threat tiers driven by settlement wealth (threat_score).
/// Higher tiers unlock more dangerous threat types and larger groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreatTier {
    /// Score 0-14: occasional lone wolf.
    Quiet,
    /// Score 15-29: wolf packs of 2-3.
    Growing,
    /// Score 30-49: scout raiders appear.
    Established,
    /// Score 50-74: raiding parties.
    Prosperous,
    /// Score 75+: coordinated assaults.
    Empire,
}

impl ThreatTier {
    pub fn from_score(score: f64) -> Self {
        if score >= 75.0 {
            ThreatTier::Empire
        } else if score >= 50.0 {
            ThreatTier::Prosperous
        } else if score >= 30.0 {
            ThreatTier::Established
        } else if score >= 15.0 {
            ThreatTier::Growing
        } else {
            ThreatTier::Quiet
        }
    }
}

/// Colony-level knowledge of the world — shared across all villagers.
#[derive(Debug, Clone, Default)]
pub struct SettlementKnowledge {
    pub known_wood: Vec<(usize, usize)>,
    pub known_stone: Vec<(usize, usize)>,
    pub known_food: Vec<(usize, usize)>,
    pub frontier: Vec<(usize, usize)>,
}

// ─── Outpost system ─────────────────────────────────────────────────────────

/// Minimum distance (tiles) from the main stockpile for a tile to qualify as outpost-worthy.
pub const OUTPOST_MIN_DISTANCE: f64 = 30.0;
/// Minimum traffic on a tile before it can trigger outpost creation.
pub const OUTPOST_TRAFFIC_THRESHOLD: f64 = 20.0;
/// Minimum settlement population before outposts can be built.
pub const OUTPOST_MIN_POP: u32 = 15;
/// Minimum distance between two outposts (prevents stacking).
pub const OUTPOST_EXCLUSION_RADIUS: f64 = 25.0;
/// Ticks with no nearby gathering activity before an outpost is abandoned.
pub const OUTPOST_IDLE_TICKS: u64 = 500;

/// A satellite settlement near a distant resource deposit. Outposts emerge from
/// sustained high traffic and consist of a stockpile + shelter. Villagers naturally
/// use the closer stockpile; existing systems handle gathering, hauling, and road
/// formation without special outpost-specific AI.
#[derive(Debug, Clone)]
pub struct Outpost {
    /// Position of the outpost stockpile.
    pub stockpile_x: f64,
    pub stockpile_y: f64,
    /// Tick at which the outpost was established.
    pub established_tick: u64,
    /// Tick of the last nearby gathering activity (resets outpost idle timer).
    pub last_activity_tick: u64,
    /// Whether a walkable path to the main stockpile exists.
    pub road_intact: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventSystem {
    pub active_events: Vec<GameEvent>,
    pub event_log: Vec<String>,
}

impl EventSystem {
    /// Check if a drought event is currently active.
    pub fn has_drought(&self) -> bool {
        self.active_events
            .iter()
            .any(|e| matches!(e, GameEvent::Drought { .. }))
    }

    /// Check if a bountiful harvest event is currently active.
    pub fn has_bountiful_harvest(&self) -> bool {
        self.active_events
            .iter()
            .any(|e| matches!(e, GameEvent::BountifulHarvest { .. }))
    }

    /// Returns wolf spawn rate multiplier based on active events.
    pub fn wolf_spawn_multiplier(&self) -> f64 {
        for event in &self.active_events {
            if matches!(event, GameEvent::WolfSurge { .. }) {
                return 2.0;
            }
        }
        1.0
    }

    /// Returns movement speed multiplier (blizzard slows everyone).
    pub fn movement_multiplier(&self) -> f64 {
        for event in &self.active_events {
            if matches!(event, GameEvent::Blizzard { .. }) {
                return 0.5;
            }
        }
        1.0
    }

    fn has_event_type(&self, check: &str) -> bool {
        self.active_events.iter().any(|e| match e {
            GameEvent::Drought { .. } => check == "drought",
            GameEvent::BountifulHarvest { .. } => check == "harvest",
            GameEvent::Migration { .. } => check == "migration",
            GameEvent::WolfSurge { .. } => check == "wolf_surge",
            GameEvent::Plague { .. } => check == "plague",
            GameEvent::Blizzard { .. } => check == "blizzard",
            GameEvent::BanditRaid { .. } => check == "bandit_raid",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivSkills {
    pub farming: f64,
    pub mining: f64,
    pub woodcutting: f64,
    pub building: f64,
    pub military: f64,
}

impl Default for CivSkills {
    fn default() -> Self {
        Self {
            farming: 1.0,
            mining: 1.0,
            woodcutting: 1.0,
            building: 1.0,
            military: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SaveState {
    pub tick: u64,
    pub resources: Resources,
    pub skills: CivSkills,
    pub day_night: DayNightCycle,
    pub map: TileMap,
    pub heights: Vec<f64>,
    pub water: WaterMap,
    pub moisture: MoistureMap,
    pub vegetation: VegetationMap,
    pub influence: InfluenceMap,
    pub entities: Vec<SerializedEntity>,
    pub last_birth_tick: u64,
    pub peak_population: u32,
    pub raining: bool,
    pub auto_build: bool,
    pub sim_config: SimConfig,
    pub terrain_config: TerrainGenConfig,
    #[serde(default)]
    pub events: EventSystem,
    #[serde(default)]
    pub traffic: TrafficMap,
    #[serde(default)]
    pub danger_scent: ScentMap,
    #[serde(default)]
    pub home_scent: ScentMap,
    #[serde(default)]
    pub resource_map: Option<crate::terrain_pipeline::ResourceMap>,
}

/// Terminal chars are ~2x taller than wide. Each world tile gets this many
/// screen columns so the grid looks square.
const CELL_ASPECT: i32 = 2;

pub struct Game {
    pub target_fps: u32,
    pub tick: u64,
    pub map: TileMap,
    pub heights: Vec<f64>,
    pub water: WaterMap,
    pub moisture: MoistureMap,
    pub vegetation: VegetationMap,
    pub sim_config: SimConfig,
    pub terrain_config: TerrainGenConfig,
    pub camera: Camera,
    pub world: World,
    pub day_night: DayNightCycle,
    pub scroll_speed: i32,
    pub raining: bool,
    pub render_mode: RenderMode,
    pub paused: bool,
    pub query_mode: bool,
    pub query_cx: i32, // cursor world X
    pub query_cy: i32, // cursor world Y
    pub display_fps: Option<u32>,
    pub resources: Resources,
    pub build_mode: bool,
    pub build_cursor_x: i32,
    pub build_cursor_y: i32,
    pub selected_building: BuildingType,
    pub influence: InfluenceMap,
    pub last_birth_tick: u64,
    pub notifications: Vec<(u64, String)>,
    pub game_over: bool,
    pub peak_population: u32,
    pub auto_build: bool,
    pub skills: CivSkills,
    pub overlay: OverlayMode,
    pub events: EventSystem,
    pub traffic: TrafficMap,
    /// Danger scent: predators emit at their position, decays ~0.002/tick (half-life ~350 ticks).
    /// Villager pathfinding reads this to avoid dangerous areas.
    pub danger_scent: ScentMap,
    /// Home scent: buildings emit, creating a gradient toward the settlement.
    /// Lost/migrant villagers follow this to find home.
    pub home_scent: ScentMap,
    pub exploration: ExplorationMap,
    pub particles: Vec<Particle>,
    pub game_speed: u32,       // 1 = normal, 2 = 2x, 5 = 5x, 20 = 20x
    pub frame_count: u64,      // increments every frame regardless of speed/pause
    pub half_speed_base: bool, // when true, speed 1 runs sim every other frame
    pub soil: Vec<crate::terrain_pipeline::SoilType>,
    pub soil_fertility: SoilFertilityMap,
    pub river_mask: Vec<bool>,
    /// Discharge field from hydrology erosion — used to render rivers.
    /// Render with erf(0.4 * discharge) as blend toward water color.
    pub discharge: Vec<f64>,
    /// Full hydrology state for runtime erosion (momentum fields for meandering).
    pub hydro: crate::hydrology::HydroMap,
    pub pipeline_temperature: Vec<f64>,
    pub pipeline_slope: Vec<f64>,
    pub pipeline_moisture: Vec<f64>,
    pub resource_map: crate::terrain_pipeline::ResourceMap,
    pub knowledge: SettlementKnowledge,
    pub spatial_grid: crate::ecs::spatial::SpatialHashGrid,
    pub group_manager: crate::ecs::groups::GroupManager,
    pub ai_arrays: crate::ecs::ai_arrays::AiArrays,
    pub difficulty: DifficultyState,
    pub milestone_banner: Option<MilestoneBanner>,
    /// Tick at which current spring flood started (0 = no active flood).
    pub flood_start_tick: u64,
    /// Tiles currently flooded this spring (for tracking recede + fertility bonus).
    pub flooded_tiles: Vec<(usize, usize)>,
    /// Transient flag: set when a raid/wolf surge is repelled with zero deaths.
    pub raid_survived_clean: bool,
    /// Active fire tiles: (x, y, burn_ticks_remaining). Processed each tick for
    /// fire spread and burnout — O(fire_front), not O(map).
    pub fire_tiles: Vec<(usize, usize, u32)>,
    /// Per-tile chokepoint scores + clustered locations. Computed at world-gen,
    /// recomputed when `chokepoints_dirty` is set (e.g. building placement).
    pub chokepoint_map: chokepoint::ChokepointMap,
    /// Set to true when terrain changes (building placed/demolished) to trigger
    /// chokepoint recomputation on the next relevant tick.
    pub chokepoints_dirty: bool,
    /// World-space dirty tileset: tracks which tiles need redraw each frame.
    pub dirty: dirty::DirtyMap,
    /// Previous camera position — used to detect scrolls and mark_all().
    prev_camera_x: i32,
    prev_camera_y: i32,
    /// Flow field registry: shared precomputed direction fields for high-traffic
    /// destinations. See docs/design/pillar5_scale/flow_fields.md.
    pub flow_fields: crate::pathfinding::FlowFieldRegistry,
    /// Tick at which terrain was last modified (building, road, tree cut).
    /// Flow fields computed before this tick are stale.
    pub terrain_dirty_tick: u64,
    /// Hierarchical pathfinding navigation graph. Precomputed at world-gen,
    /// updated incrementally when terrain changes. See docs/design/pillar5_scale/hierarchical_pathfinding.md.
    pub nav_graph: NavGraph,
    /// Per-tile threat/defense data for the Threats overlay. Updated every 100 ticks.
    pub threat_map: ThreatMap,
    /// Wealth-based threat score, recomputed every 100 ticks from population,
    /// resources, and building count. Drives threat tier and spawn scaling.
    pub threat_score: f64,
    /// Tick of the last threat spawn, used to enforce a minimum cooldown between threats.
    pub last_threat_tick: u64,
    /// Wind vector field: terrain-deflected prevailing wind with curl noise.
    /// Recomputed on seasonal direction changes.
    pub wind: WindField,
    /// Active outposts — satellite settlements near distant resources.
    pub outposts: Vec<Outpost>,
    /// Pipe-model water simulation: 8-directional flow driven by hydrostatic pressure.
    pub pipe_water: crate::pipe_water::PipeWater,
    #[cfg(feature = "lua")]
    pub script_engine: Option<crate::scripting::ScriptEngine>,
}

/// Traffic above this threshold converts walkable terrain to road.
const ROAD_TRAFFIC_THRESHOLD: f64 = 300.0;

impl Game {
    pub fn new(target_fps: u32, seed: u32) -> Self {
        Self::new_with_size(target_fps, seed, 256, 256)
    }

    pub fn new_with_size(target_fps: u32, seed: u32, map_width: usize, map_height: usize) -> Self {
        use crate::terrain_pipeline::{PipelineConfig, run_pipeline};

        // Run the full terrain pipeline
        let mut pipeline_config = PipelineConfig::default();
        pipeline_config.terrain.seed = seed;
        let result = run_pipeline(map_width, map_height, &pipeline_config);
        Self::from_pipeline_result(target_fps, seed, result)
    }

    /// Create a game from a pre-built pipeline result (e.g. from --live-gen).
    /// Skips running the pipeline — uses the provided heights, discharge, etc.
    pub fn new_from_pipeline(
        target_fps: u32,
        seed: u32,
        result: crate::terrain_pipeline::PipelineResult,
    ) -> Self {
        Self::from_pipeline_result(target_fps, seed, result)
    }

    fn from_pipeline_result(
        target_fps: u32,
        seed: u32,
        result: crate::terrain_pipeline::PipelineResult,
    ) -> Self {
        let map_width = result.map.width;
        let map_height = result.map.height;
        let terrain_config = crate::terrain_gen::TerrainGenConfig {
            seed,
            ..Default::default()
        };

        let mut map = result.map;
        let heights = result.heights;

        // Seed water from pipeline rivers + water tiles
        let mut water = WaterMap::new(map_width, map_height);
        // Only seed old WaterMap on actual Water terrain (river_mask disabled)
        for y in 0..map_height {
            for x in 0..map_width {
                if matches!(map.get(x, y), Some(Terrain::Water)) {
                    let i = y * map_width + x;
                    let depth = (terrain_config.water_level - heights[i]).max(0.01);
                    water.set(x, y, depth);
                }
            }
        }
        let mut moisture = MoistureMap::new(map_width, map_height);
        // Initialize both current and average moisture from pipeline so vegetation
        // starts sensible and the runtime moisture isn't stuck at zero.
        moisture.moisture = result.moisture.clone();
        moisture.avg_moisture = result.moisture.clone();
        let mut vegetation = VegetationMap::new(map_width, map_height);

        // Seed vegetation from biome so the map looks alive from tick 0
        for y in 0..map_height {
            for x in 0..map_width {
                let veg = match map.get(x, y) {
                    Some(Terrain::Forest) => 0.8,
                    Some(Terrain::Grass) => 0.4,
                    Some(Terrain::Scrubland) => 0.2,
                    Some(Terrain::Marsh) => 0.5,
                    Some(Terrain::Sapling) => 0.3,
                    Some(Terrain::Sand) => 0.05,
                    Some(Terrain::Tundra) => 0.1,
                    Some(Terrain::Desert) => 0.02,
                    _ => 0.0,
                };
                if let Some(v) = vegetation.get_mut(x, y) {
                    *v = veg;
                }
            }
        }

        let camera = Camera::new(100, 100);
        let mut world = World::new();

        // Spawn entities on walkable tiles (search outward if blocked)
        let find_walkable = |map: &TileMap, cx: usize, cy: usize| -> (f64, f64) {
            for r in 0..50 {
                for dy in -(r as i32)..=(r as i32) {
                    for dx in -(r as i32)..=(r as i32) {
                        if dx.unsigned_abs() as usize != r && dy.unsigned_abs() as usize != r {
                            continue;
                        }
                        let x = cx as i32 + dx;
                        let y = cy as i32 + dy;
                        if map.is_walkable(x as f64, y as f64) {
                            return (x as f64, y as f64);
                        }
                    }
                }
            }
            (cx as f64, cy as f64) // fallback
        };

        // Find a good start position: grass/sand tile adjacent to forest, near map center.
        // Prefer locations near fords (settlements near natural crossings have geographic advantage).
        // Also require at least 5 distinct 3×3 buildable areas within 20 tiles so auto-build
        let cx = map_width / 2;
        let cy = map_height / 2;
        let mut start_cx = cx;
        let mut start_cy = cy;
        let mut used_fallback = false;

        // Helper: check if a ford exists within `radius` tiles of (ux, uy)
        let has_ford_nearby = |map_ref: &TileMap, ux: usize, uy: usize, radius: i32| -> bool {
            for fy in -radius..=radius {
                for fx in -radius..=radius {
                    let nx = ux as i32 + fx;
                    let ny = uy as i32 + fy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < map_width && (ny as usize) < map_height
                    {
                        if map_ref.get(nx as usize, ny as usize) == Some(&Terrain::Ford) {
                            return true;
                        }
                    }
                }
            }
            false
        };

        // Helper: count buildable 3x3 zones within scan_r
        let count_buildable_zones =
            |map_ref: &TileMap, ux: usize, uy: usize, scan_r: i32| -> usize {
                let mut count = 0usize;
                let mut gx = ux as i32 - scan_r;
                while gx <= ux as i32 + scan_r - 2 {
                    let mut gy = uy as i32 - scan_r;
                    while gy <= uy as i32 + scan_r - 2 {
                        let zone_fits = (0..3i32).all(|fy| {
                            (0..3i32).all(|fx| {
                                let tx = (gx + fx).max(0) as usize;
                                let ty = (gy + fy).max(0) as usize;
                                matches!(map_ref.get(tx, ty), Some(Terrain::Grass | Terrain::Sand))
                            })
                        });
                        if zone_fits {
                            count += 1;
                        }
                        gy += 3;
                    }
                    gx += 3;
                }
                count
            };

        // First pass: prefer spawn near a ford (within 20 tiles) for geographic advantage
        'ford_search: for r in 0..80usize {
            for dy in -(r as i32)..=(r as i32) {
                for dx in -(r as i32)..=(r as i32) {
                    if (dx.unsigned_abs() as usize != r) && (dy.unsigned_abs() as usize != r) {
                        continue;
                    }
                    let x = cx as i32 + dx;
                    let y = cy as i32 + dy;
                    if x < 2 || y < 2 || x as usize >= map_width - 2 || y as usize >= map_height - 2
                    {
                        continue;
                    }
                    let ux = x as usize;
                    let uy = y as usize;
                    if let Some(t) = map.get(ux, uy)
                        && matches!(t, Terrain::Grass | Terrain::Sand)
                        && has_ford_nearby(&map, ux, uy, 20)
                    {
                        let has_forest = (-3i32..=3).any(|fy| {
                            (-3i32..=3).any(|fx| {
                                map.get((ux as i32 + fx) as usize, (uy as i32 + fy) as usize)
                                    == Some(&Terrain::Forest)
                            })
                        });
                        if has_forest && count_buildable_zones(&map, ux, uy, 25) >= 8 {
                            start_cx = ux;
                            start_cy = uy;
                            break 'ford_search;
                        }
                    }
                }
            }
        }

        // Second pass (normal): if ford search didn't find anything, use standard search
        if start_cx == cx && start_cy == cy {
            'search: for r in 0..80usize {
                for dy in -(r as i32)..=(r as i32) {
                    for dx in -(r as i32)..=(r as i32) {
                        if (dx.unsigned_abs() as usize != r) && (dy.unsigned_abs() as usize != r) {
                            continue;
                        }
                        let x = cx as i32 + dx;
                        let y = cy as i32 + dy;
                        if x < 2
                            || y < 2
                            || x as usize >= map_width - 2
                            || y as usize >= map_height - 2
                        {
                            continue;
                        }
                        let ux = x as usize;
                        let uy = y as usize;
                        if let Some(t) = map.get(ux, uy)
                            && matches!(t, Terrain::Grass | Terrain::Sand)
                        {
                            // Check if forest is adjacent (within 3 tiles)
                            let has_forest = (-3i32..=3).any(|fy| {
                                (-3i32..=3).any(|fx| {
                                    map.get((ux as i32 + fx) as usize, (uy as i32 + fy) as usize)
                                        == Some(&Terrain::Forest)
                                })
                            });
                            if has_forest {
                                // Count non-overlapping 3×3 Grass/Sand zones within 25 tiles
                                // (step by 3 in both axes). Need ≥8 to reject narrow corridors:
                                // a 3-wide corridor has zones only along one axis (e.g. 8 zones
                                // requires a 24-long corridor), but a genuine open area (9×9)
                                // provides 9 zones spread in both dimensions. The higher threshold
                                // ensures auto-build can place multiple distinct buildings without
                                // running out of valid spots, and rejects the seed 137 narrow
                                // mountain corridor that has been blocking pop growth every session.
                                let mut buildable_count = 0usize;
                                let scan_r = 25i32;
                                let mut gx = ux as i32 - scan_r;
                                while gx <= ux as i32 + scan_r - 2 {
                                    let mut gy = uy as i32 - scan_r;
                                    while gy <= uy as i32 + scan_r - 2 {
                                        let zone_fits = (0..3i32).all(|fy| {
                                            (0..3i32).all(|fx| {
                                                let tx = (gx + fx).max(0) as usize;
                                                let ty = (gy + fy).max(0) as usize;
                                                matches!(
                                                    map.get(tx, ty),
                                                    Some(Terrain::Grass | Terrain::Sand)
                                                )
                                            })
                                        });
                                        if zone_fits {
                                            buildable_count += 1;
                                        }
                                        gy += 3;
                                    }
                                    gx += 3;
                                }
                                if buildable_count >= 8 {
                                    start_cx = ux;
                                    start_cy = uy;
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
        } // end if start_cx == cx (normal search fallback)

        // Fallback spawn search: if no grass+forest+8zones position was found (happens on maps
        // where open grass areas aren't adjacent to forest, e.g. seed 137 desert), accept any
        // Grass/Sand tile with ≥8 buildable zones even without forest adjacency. Villagers can
        // still gather wood from forests up to 22 tiles away via sight_range.
        if start_cx == cx && start_cy == cy {
            'fallback: for r in 0..80usize {
                for dy in -(r as i32)..=(r as i32) {
                    for dx in -(r as i32)..=(r as i32) {
                        if (dx.unsigned_abs() as usize != r) && (dy.unsigned_abs() as usize != r) {
                            continue;
                        }
                        let x = cx as i32 + dx;
                        let y = cy as i32 + dy;
                        if x < 2
                            || y < 2
                            || x as usize >= map_width - 2
                            || y as usize >= map_height - 2
                        {
                            continue;
                        }
                        let ux = x as usize;
                        let uy = y as usize;
                        if matches!(map.get(ux, uy), Some(Terrain::Grass | Terrain::Sand)) {
                            let mut buildable_count = 0usize;
                            let scan_r = 25i32;
                            let mut gx = ux as i32 - scan_r;
                            while gx <= ux as i32 + scan_r - 2 {
                                let mut gy = uy as i32 - scan_r;
                                while gy <= uy as i32 + scan_r - 2 {
                                    let zone_fits = (0..3i32).all(|fy| {
                                        (0..3i32).all(|fx| {
                                            let tx = (gx + fx).max(0) as usize;
                                            let ty = (gy + fy).max(0) as usize;
                                            matches!(
                                                map.get(tx, ty),
                                                Some(Terrain::Grass | Terrain::Sand)
                                            )
                                        })
                                    });
                                    if zone_fits {
                                        buildable_count += 1;
                                    }
                                    gy += 3;
                                }
                                gx += 3;
                            }
                            if buildable_count >= 8 {
                                start_cx = ux;
                                start_cy = uy;
                                used_fallback = true;
                                break 'fallback;
                            }
                        }
                    }
                }
            }
        }

        // Fallback forest planting: when the fallback spawn search was used (no nearby forest),
        // plant 4-6 Forest tiles within 5-8 tiles of spawn so villagers have a wood source.
        // Think of it as settlers choosing a spot near a small copse of trees.
        if used_fallback {
            let mut rng = rand::rng();
            let trees_to_plant = rng.random_range(4u32..=6);
            let mut planted = 0u32;
            for _ in 0..200 {
                if planted >= trees_to_plant {
                    break;
                }
                let angle = rng.random_range(0.0f64..std::f64::consts::TAU);
                let radius = rng.random_range(5.0f64..8.0);
                let tx = (start_cx as f64 + angle.cos() * radius).round() as i32;
                let ty = (start_cy as f64 + angle.sin() * radius).round() as i32;
                if tx < 0 || ty < 0 || tx as usize >= map_width || ty as usize >= map_height {
                    continue;
                }
                let ux = tx as usize;
                let uy = ty as usize;
                if matches!(map.get(ux, uy), Some(Terrain::Grass | Terrain::Sand)) {
                    map.set(ux, uy, Terrain::Forest);
                    planted += 1;
                }
            }
        }

        // Wildlife: spawn 3 dens with 2 prey each in forest/grass tiles 8-50 tiles from center.
        // Prey provide an early food web and are required for the breeding system to function
        // (breeding needs at least 1 existing prey per den; 0 prey = 0 breeding = permanent extinction).
        {
            let mut dens_placed = 0usize;
            let mut rng = rand::rng();
            'den_search: for r in 8usize..50 {
                for _ in 0..12 {
                    let angle = rng.random_range(0.0f64..std::f64::consts::TAU);
                    let rx = (cx as i32 + (angle.cos() * r as f64) as i32)
                        .clamp(1, map_width as i32 - 2);
                    let ry = (cy as i32 + (angle.sin() * r as f64) as i32)
                        .clamp(1, map_height as i32 - 2);
                    if let Some(t) = map.get(rx as usize, ry as usize) {
                        if matches!(t, Terrain::Forest | Terrain::Grass)
                            && map.is_walkable(rx as f64, ry as f64)
                        {
                            let dx = rx as f64;
                            let dy = ry as f64;
                            ecs::spawn_den(&mut world, dx, dy);
                            for _ in 0..2 {
                                let mut prey_spawned = false;
                                for _ in 0..20 {
                                    let px = dx + rng.random_range(-3.0f64..3.0);
                                    let py = dy + rng.random_range(-3.0f64..3.0);
                                    if map.is_walkable(px, py) {
                                        ecs::spawn_prey(&mut world, px, py, dx, dy);
                                        prey_spawned = true;
                                        break;
                                    }
                                }
                                let _ = prey_spawned;
                            }
                            dens_placed += 1;
                            if dens_placed >= 3 {
                                break 'den_search;
                            }
                        }
                    }
                }
            }
        }

        // Settlement: stockpile + villagers near found start position
        let scx = start_cx;
        let scy = start_cy;

        // Helper: find a spot where an NxM building fits on natural terrain (no buildings)
        // with a 1-tile walkable gap around the footprint so buildings don't block each other.
        // Prefers Grass/Sand positions to avoid consuming Forest tiles that villagers need for
        // wood gathering. Falls back to allowing Forest only if no Grass/Sand spot exists.
        let find_building_spot = |map: &TileMap,
                                  cx: usize,
                                  cy: usize,
                                  bw: usize,
                                  bh: usize|
         -> (f64, f64) {
            // Check that all footprint tiles match the terrain predicate AND a 1-tile
            // border around the footprint has no BuildingFloor/BuildingWall tiles.
            let has_gap = |map: &TileMap, x: i32, y: i32, bw: i32, bh: i32| -> bool {
                for fy in -1..bh + 1 {
                    for fx in -1..bw + 1 {
                        // Skip interior — only check the 1-tile border
                        if fx >= 0 && fx < bw && fy >= 0 && fy < bh {
                            continue;
                        }
                        let tx = x + fx;
                        let ty = y + fy;
                        if tx < 0 || ty < 0 {
                            continue; // map edge is fine
                        }
                        if matches!(
                            map.get(tx as usize, ty as usize),
                            Some(Terrain::BuildingFloor | Terrain::BuildingWall)
                        ) {
                            return false;
                        }
                    }
                }
                true
            };

            // First pass: only Grass/Sand (preserve nearby forest for wood gathering)
            for r in 0..30usize {
                for dy in -(r as i32)..=(r as i32) {
                    for dx in -(r as i32)..=(r as i32) {
                        if (dx.unsigned_abs() as usize != r) && (dy.unsigned_abs() as usize != r) {
                            continue;
                        }
                        let x = cx as i32 + dx;
                        let y = cy as i32 + dy;
                        if x < 0 || y < 0 {
                            continue;
                        }
                        let fits = (0..bh as i32).all(|fy| {
                            (0..bw as i32).all(|fx| {
                                let tx = (x + fx) as usize;
                                let ty = (y + fy) as usize;
                                matches!(map.get(tx, ty), Some(Terrain::Grass | Terrain::Sand))
                            })
                        });
                        if fits && has_gap(map, x, y, bw as i32, bh as i32) {
                            return (x as f64, y as f64);
                        }
                    }
                }
            }
            // Second pass: allow Forest as fallback (better than placing on impassable terrain)
            for r in 0..30usize {
                for dy in -(r as i32)..=(r as i32) {
                    for dx in -(r as i32)..=(r as i32) {
                        if (dx.unsigned_abs() as usize != r) && (dy.unsigned_abs() as usize != r) {
                            continue;
                        }
                        let x = cx as i32 + dx;
                        let y = cy as i32 + dy;
                        if x < 0 || y < 0 {
                            continue;
                        }
                        let fits = (0..bh as i32).all(|fy| {
                            (0..bw as i32).all(|fx| {
                                let tx = (x + fx) as usize;
                                let ty = (y + fy) as usize;
                                matches!(
                                    map.get(tx, ty),
                                    Some(Terrain::Grass | Terrain::Sand | Terrain::Forest)
                                )
                            })
                        });
                        if fits && has_gap(map, x, y, bw as i32, bh as i32) {
                            return (x as f64, y as f64);
                        }
                    }
                }
            }
            (cx as f64, cy as f64)
        };

        // Place stockpile (2x2)
        let (sx, sy) = find_building_spot(&map, scx, scy, 2, 2);
        ecs::spawn_stockpile(&mut world, sx, sy);
        for dy in 0..2 {
            for dx in 0..2 {
                map.set(sx as usize + dx, sy as usize + dy, Terrain::BuildingFloor);
            }
        }

        // Pre-built hut — search offset from stockpile, tiles already marked prevent overlap
        let (hsw, hsh) = BuildingType::Hut.size();
        let (hx, hy) = find_building_spot(
            &map,
            scx.wrapping_sub(4),
            scy.wrapping_sub(3),
            hsw as usize,
            hsh as usize,
        );
        if (hx as usize) < map_width && (hy as usize) < map_height {
            for (dx, dy, terrain) in BuildingType::Hut.tiles() {
                let tx = hx as i32 + dx;
                let ty = hy as i32 + dy;
                if tx >= 0 && ty >= 0 && (tx as usize) < map_width && (ty as usize) < map_height {
                    map.set(tx as usize, ty as usize, terrain);
                }
            }
            ecs::spawn_hut(&mut world, hx + hsw as f64 / 2.0, hy + hsh as f64 / 2.0);
        }

        // Pre-built farm — search opposite side of stockpile
        let (fsw, fsh) = BuildingType::Farm.size();
        let (fx, fy) = find_building_spot(
            &map,
            scx + 4,
            scy.wrapping_sub(3),
            fsw as usize,
            fsh as usize,
        );
        if (fx as usize) < map_width && (fy as usize) < map_height {
            for (dx, dy, terrain) in BuildingType::Farm.tiles() {
                let tx = fx as i32 + dx;
                let ty = fy as i32 + dy;
                if tx >= 0 && ty >= 0 && (tx as usize) < map_width && (ty as usize) < map_height {
                    map.set(tx as usize, ty as usize, terrain);
                }
            }
            ecs::spawn_farm_plot(&mut world, fx + fsw as f64 / 2.0, fy + fsh as f64 / 2.0);
        }

        // Pre-built Granary — converts food to grain which is preserved through Winter.
        // Without this, winter food decay (2%/30 ticks) kills all settlements before Year 2.
        let (gsw, gsh) = BuildingType::Granary.size();
        let (gx, gy) = find_building_spot(&map, scx + 5, scy + 4, gsw as usize, gsh as usize);
        if (gx as usize) < map_width && (gy as usize) < map_height {
            for (dx, dy, terrain) in BuildingType::Granary.tiles() {
                let tx = gx as i32 + dx;
                let ty = gy as i32 + dy;
                if tx >= 0 && ty >= 0 && (tx as usize) < map_width && (ty as usize) < map_height {
                    map.set(tx as usize, ty as usize, terrain);
                }
            }
            ecs::spawn_processing_building(
                &mut world,
                gx + gsw as f64 / 2.0,
                gy + gsh as f64 / 2.0,
                Recipe::FoodToGrain,
            );
        }

        // Spawn berry bushes from ResourceMap: pick the 2 best (fertility / distance)
        // tiles within 8 tiles of settlement that are walkable.
        {
            let mut berry_candidates: Vec<(u32, usize, usize)> = Vec::new();
            let scan_r = 8i32;
            for dy in -scan_r..=scan_r {
                for dx in -scan_r..=scan_r {
                    let tx = scx as i32 + dx;
                    let ty = scy as i32 + dy;
                    if tx < 0 || ty < 0 || tx >= map_width as i32 || ty >= map_height as i32 {
                        continue;
                    }
                    let ux = tx as usize;
                    let uy = ty as usize;
                    let pot = result.resources.get(ux, uy);
                    if pot.fertility > 0 && map.is_walkable(tx as f64, ty as f64) {
                        // Score: fertility weighted by proximity (closer = better)
                        let dist = (dx.abs() + dy.abs()).max(1) as u32;
                        let score = pot.fertility as u32 * 10 / dist;
                        berry_candidates.push((score, ux, uy));
                    }
                }
            }
            berry_candidates.sort_by(|a, b| b.0.cmp(&a.0));
            // Spatial sampling: skip candidates within 3 tiles of an already-placed bush
            let mut placed_bushes: Vec<(usize, usize)> = Vec::new();
            for (_, bx, by) in &berry_candidates {
                if placed_bushes.len() >= 2 {
                    break;
                }
                let too_close = placed_bushes.iter().any(|(px, py)| {
                    let ddx = (*bx as i32 - *px as i32).abs();
                    let ddy = (*by as i32 - *py as i32).abs();
                    ddx + ddy < 3
                });
                if !too_close {
                    ecs::spawn_berry_bush(&mut world, *bx as f64, *by as f64);
                    placed_bushes.push((*bx, *by));
                }
            }
            // Fallback: if no high-fertility tile found, use old approach
            if placed_bushes.is_empty() {
                for &(bsx, bsy) in &[
                    (scx.wrapping_sub(1), scy.wrapping_sub(1)),
                    (scx + 1, scy + 2),
                ] {
                    let (bx, by) = find_walkable(&map, bsx, bsy);
                    ecs::spawn_berry_bush(&mut world, bx, by);
                }
            }
        }

        // Spawn stone deposits from ResourceMap: pick the 2 best (stone / distance)
        // tiles within 12 tiles of settlement that are walkable.
        {
            let mut stone_candidates: Vec<(u32, usize, usize)> = Vec::new();
            let scan_r = 12i32;
            for dy in -scan_r..=scan_r {
                for dx in -scan_r..=scan_r {
                    let tx = scx as i32 + dx;
                    let ty = scy as i32 + dy;
                    if tx < 0 || ty < 0 || tx >= map_width as i32 || ty >= map_height as i32 {
                        continue;
                    }
                    let ux = tx as usize;
                    let uy = ty as usize;
                    let pot = result.resources.get(ux, uy);
                    if pot.stone > 0 && map.is_walkable(tx as f64, ty as f64) {
                        let dist = (dx.abs() + dy.abs()).max(1) as u32;
                        let score = pot.stone as u32 * 10 / dist;
                        stone_candidates.push((score, ux, uy));
                    }
                }
            }
            stone_candidates.sort_by(|a, b| b.0.cmp(&a.0));
            let mut placed_deposits: Vec<(usize, usize)> = Vec::new();
            for (_, sx_c, sy_c) in &stone_candidates {
                if placed_deposits.len() >= 2 {
                    break;
                }
                let too_close = placed_deposits.iter().any(|(px, py)| {
                    let ddx = (*sx_c as i32 - *px as i32).abs();
                    let ddy = (*sy_c as i32 - *py as i32).abs();
                    ddx + ddy < 3
                });
                if !too_close {
                    ecs::spawn_stone_deposit(&mut world, *sx_c as f64, *sy_c as f64);
                    placed_deposits.push((*sx_c, *sy_c));
                }
            }
            // Fallback: if no stone-rich tile found, use old approach
            if placed_deposits.is_empty() {
                for &(dsx, dsy) in &[(scx.wrapping_sub(3), scy), (scx + 3, scy + 1)] {
                    let (ddx, ddy) = find_walkable(&map, dsx, dsy);
                    ecs::spawn_stone_deposit(&mut world, ddx, ddy);
                }
            }
        }

        // Spawn 3 prey dens 8-40 tiles from settlement center (forest/grass tiles preferred).
        // Prey provide early food variety and establish the predator/prey ecosystem.
        // Without initial prey, dens never get populated and rabbits remain at 0 forever.
        // Wide search radius (8-40 tiles, 150 attempts) handles hostile terrain like mountains
        // and water-heavy maps where the 8-25 range may have few walkable tiles.
        {
            let mut rng = rand::rng();
            let scx_f = scx as f64;
            let scy_f = scy as f64;
            let mut dens_placed = 0u32;
            for _ in 0..150 {
                if dens_placed >= 3 {
                    break;
                }
                let angle = rng.random_range(0.0f64..std::f64::consts::TAU);
                let dist = rng.random_range(8.0f64..40.0);
                let px = scx_f + angle.cos() * dist;
                let py = scy_f + angle.sin() * dist;
                if px >= 0.0 && py >= 0.0 && map.is_walkable(px, py) {
                    ecs::spawn_den(&mut world, px, py);
                    ecs::spawn_prey(&mut world, px + 1.0, py, px, py);
                    ecs::spawn_prey(&mut world, px - 1.0, py, px, py);
                    dens_placed += 1;
                }
            }
        }

        // Spawn 3 villagers near the stockpile (staggered to avoid tick-sync)
        for i in 0..3 {
            let (vx, vy) = find_walkable(&map, scx + i * 2, scy + 1);
            ecs::spawn_villager_staggered(&mut world, vx, vy, 0);
        }

        // Snapshot base terrain before any seasonal effects
        map.init_base_terrain();

        let mut g = Self {
            target_fps,
            tick: 0,
            map,
            heights,
            water,
            moisture,
            vegetation,
            sim_config: SimConfig::default(),
            terrain_config,
            camera,
            world,
            day_night: DayNightCycle::new(map_width, map_height),
            scroll_speed: 2,
            raining: false,
            paused: false,
            render_mode: RenderMode::Normal,
            query_mode: false,
            query_cx: scx as i32,
            query_cy: scy as i32,
            display_fps: None,
            resources: Resources {
                food: 20,
                wood: 60,
                stone: 20,
                ..Default::default()
            },
            build_mode: false,
            build_cursor_x: scx as i32,
            build_cursor_y: scy as i32,
            selected_building: BuildingType::Wall,
            influence: InfluenceMap::new(map_width, map_height),
            last_birth_tick: 0,
            notifications: Vec::new(),
            game_over: false,
            peak_population: 3,
            auto_build: false,
            skills: CivSkills::default(),
            overlay: OverlayMode::None,
            events: EventSystem::default(),
            traffic: TrafficMap::new(map_width, map_height),
            // Danger scent: decay_rate 0.990 applied every 5 ticks → half-life ~350 ticks.
            // Spread factor 0.06 — danger feels larger than the exact spot.
            danger_scent: ScentMap::new(map_width, map_height, 0.990, 0.06),
            // Home scent: decay_rate 0.998 applied every 10 ticks → half-life ~3500 ticks.
            // Spread factor 0.08 — radiates broadly from settlement.
            home_scent: ScentMap::new(map_width, map_height, 0.998, 0.08),
            exploration: ExplorationMap::new(map_width, map_height),
            particles: Vec::new(),
            game_speed: 1,
            frame_count: 0,
            half_speed_base: false,
            soil_fertility: SoilFertilityMap::from_soil_types(map_width, map_height, &result.soil),
            soil: result.soil,
            river_mask: result.river_mask,
            discharge: {
                let max_d = result.discharge.iter().cloned().fold(0.0f64, f64::max);
                let avg_d = result.discharge.iter().sum::<f64>() / result.discharge.len().max(1) as f64;
                let visible = result.discharge.iter().filter(|&&d| crate::hydrology::erf_approx(0.4 * d) > 0.1).count();
                eprintln!("[HYDROLOGY] discharge: max={max_d:.4} avg={avg_d:.6} visible_tiles={visible}/{}", result.discharge.len());
                result.discharge
            },
            hydro: result.hydro,
            pipeline_temperature: result.temperature,
            pipeline_slope: result.slope,
            pipeline_moisture: result.moisture,
            resource_map: result.resources,
            knowledge: SettlementKnowledge::default(),
            spatial_grid: crate::ecs::spatial::SpatialHashGrid::new(map_width, map_height, 16),
            group_manager: crate::ecs::groups::GroupManager::new(),
            ai_arrays: crate::ecs::ai_arrays::AiArrays::new(64),
            difficulty: DifficultyState::default(),
            milestone_banner: None,
            flood_start_tick: 0,
            flooded_tiles: Vec::new(),
            raid_survived_clean: false,
            fire_tiles: Vec::new(),
            chokepoint_map: chokepoint::ChokepointMap::empty(map_width, map_height),
            chokepoints_dirty: true, // will be computed on first access
            dirty: dirty::DirtyMap::new(map_width, map_height),
            prev_camera_x: i32::MIN, // force mark_all on first frame
            prev_camera_y: i32::MIN,
            flow_fields: crate::pathfinding::FlowFieldRegistry::new(),
            terrain_dirty_tick: 0,
            nav_graph: NavGraph::default(), // rebuilt below
            threat_map: ThreatMap::new(map_width, map_height),
            threat_score: 0.0,
            last_threat_tick: 0,
            wind: WindField::new(map_width, map_height), // rebuilt below
            outposts: Vec::new(),
            pipe_water: crate::pipe_water::PipeWater::new(map_width, map_height),
            #[cfg(feature = "lua")]
            script_engine: None,
        };

        // Seed pipe_water from two sources:
        // 1. Ocean tiles (Terrain::Water) — boundary condition, constant depth
        // 2. Discharge field — high-discharge channels get actual water depth
        //    so rivers are visible from tick 0 via the pipe_water renderer.
        let pipeline_wl = result.water_level;
        for y in 0..map_height {
            for x in 0..map_width {
                let i = y * map_width + x;
                if matches!(g.map.get(x, y), Some(Terrain::Water)) {
                    let depth = (pipeline_wl - g.heights[i]).max(0.01);
                    g.pipe_water.add_water(x, y, depth);
                    g.pipe_water.set_ocean_boundary(x, y, depth);
                } else if i < g.discharge.len() {
                    let d = crate::hydrology::erf_approx(0.4 * g.discharge[i]);
                    if d > 0.5 {
                        // Strong river channel — thin water layer
                        g.pipe_water.add_water(x, y, (d - 0.5) * 0.02);
                    }
                }
            }
        }

        // Compute initial chokepoint map from generated terrain
        g.chokepoint_map = chokepoint::ChokepointMap::compute(&g.map, &g.river_mask);
        g.chokepoints_dirty = false;
        // Compute initial wind field from terrain + chokepoints
        let wind_dir = WindField::seasonal_direction(g.day_night.season);
        g.wind = match g.sim_config.wind_model {
            crate::simulation::WindModel::CurlNoise => WindField::compute_curl_noise_field(
                &g.heights,
                map_width,
                map_height,
                wind_dir,
                0.6,
                0.0, // time=0 at start
                g.terrain_config.seed,
            ),
            crate::simulation::WindModel::Stam => WindField::compute_from_terrain(
                &g.heights,
                map_width,
                map_height,
                wind_dir,
                0.6,
                Some(&g.chokepoint_map.scores),
            ),
        };
        // Build hierarchical pathfinding navigation graph from final terrain
        g.nav_graph = NavGraph::build(&g.map);
        // Pre-reveal settlement start area (around map center)
        g.exploration.reveal(scx, scy, 15);
        // Start camera at settlement
        // Center camera on settlement (will be clamped after first step)
        g.camera.x = scx as i32 - 30;
        g.camera.y = scy as i32 - 23;
        g.notify("Settlement founded! [b]uild, [k]query, arrows scroll".to_string());
        g
    }

    pub fn notify(&mut self, msg: String) {
        self.notifications.push((self.tick, msg));
        // Keep only last 5 notifications
        if self.notifications.len() > 5 {
            self.notifications.remove(0);
        }
    }

    /// Fire a milestone notification: set banner, push to event log with [*] prefix,
    /// and add to regular notifications.
    pub fn notify_milestone(&mut self, msg: &str) {
        self.milestone_banner = Some(MilestoneBanner {
            message: msg.to_string(),
            ticks_remaining: 120,
        });
        self.events.event_log.push(format!("[*] {}", msg));
        self.notify(format!("[*] {}", msg));
    }

    /// Load all .lua scripts from a directory into the script engine.
    /// Creates a new ScriptEngine if one doesn't exist yet.
    #[cfg(feature = "lua")]
    pub fn load_scripts(&mut self, dir: &str) -> Result<()> {
        let engine = match self.script_engine.take() {
            Some(e) => e,
            None => crate::scripting::ScriptEngine::new()
                .map_err(|e| anyhow::anyhow!("Failed to create ScriptEngine: {}", e))?,
        };
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "lua") {
                    if let Err(e) = engine.load_script(&path.to_string_lossy()) {
                        eprintln!("Lua script error in {:?}: {}", path, e);
                    }
                }
            }
        }
        self.script_engine = Some(engine);
        Ok(())
    }

    /// Call lua on_tick hook with current game state.
    #[cfg(feature = "lua")]
    fn lua_on_tick(&self) {
        if let Some(ref engine) = self.script_engine {
            let villager_count = self
                .world
                .query::<&Creature>()
                .iter()
                .filter(|c| c.species == Species::Villager)
                .count() as u32;
            let wolf_count = self
                .world
                .query::<&Creature>()
                .iter()
                .filter(|c| c.species == Species::Predator)
                .count() as u32;
            let season_name = self.day_night.season.name();
            let _ = engine.update_state(villager_count, &self.resources, season_name, wolf_count);
            let _ = engine.call_hook("on_tick");
        }
    }

    /// Hot-reload lua scripts from the scripts/ directory every 100 ticks.
    #[cfg(feature = "lua")]
    fn lua_hot_reload(&mut self) {
        if self.tick % 100 == 0 {
            if std::path::Path::new("scripts").is_dir() {
                if let Some(ref engine) = self.script_engine {
                    let _ = engine.reload_scripts("scripts");
                } else {
                    // Auto-initialize engine if scripts/ directory exists
                    let _ = self.load_scripts("scripts");
                }
            }
        }
    }

    pub fn step(&mut self, input: GameInput, renderer: &mut dyn Renderer) -> Result<()> {
        // In game-over state, only allow quit/restart
        if self.game_over {
            match input {
                GameInput::Quit | GameInput::Restart | GameInput::None => {}
                _ => {
                    // Still render the game-over screen
                    let (vw, vh) = renderer.size();
                    let map_w = vw.saturating_sub(PANEL_WIDTH);
                    let world_vw = (map_w as i32 / CELL_ASPECT) as u16;
                    self.camera
                        .clamp(self.map.width, self.map.height, world_vw, vh);
                    self.dirty.mark_all(); // game-over screen always redraws
                    renderer.clear();
                    self.draw(renderer);
                    self.draw_game_over(renderer);
                    renderer.flush()?;
                    self.dirty.clear();
                    return Ok(());
                }
            }
        }

        // input
        self.handle_input(input, renderer);

        let (vw, vh) = renderer.size();
        // World-space viewport: map area is screen minus panel, divided by aspect ratio
        let map_w = vw.saturating_sub(PANEL_WIDTH);
        let world_vw = (map_w as i32 / CELL_ASPECT) as u16;
        self.camera
            .clamp(self.map.width, self.map.height, world_vw, vh);

        // update simulation (skip when paused)
        // At speed 1 with half_speed_base enabled, sim runs every other frame
        // for a more deliberate pace (still 60fps rendering). Speed 2+ runs normally.
        // StepOneTick forces exactly 1 tick even when paused.
        self.frame_count += 1;
        let step_one = input == GameInput::StepOneTick;
        let skip_this_frame =
            self.game_speed == 1 && self.half_speed_base && self.frame_count % 2 != 0;
        let ticks_this_frame = if step_one { 1 } else { self.game_speed };
        if step_one || (!self.paused && !skip_this_frame) {
            for _speed_tick in 0..ticks_this_frame {
                self.tick += 1;

                // Safety teleport: rescue entities stranded on impassable tiles
                // (e.g. after Water became impassable). Teleport to nearest walkable tile.
                {
                    let mut teleports: Vec<(hecs::Entity, f64, f64)> = Vec::new();
                    for (entity, pos) in self.world.query::<(hecs::Entity, &Position)>().iter() {
                        if !self.map.is_walkable(pos.x, pos.y) {
                            if let Some((nx, ny)) = self.map.find_nearest_walkable(pos.x, pos.y) {
                                teleports.push((entity, nx, ny));
                            }
                        }
                    }
                    for (entity, nx, ny) in teleports {
                        if let Ok(mut pos) = self.world.get::<&mut Position>(entity) {
                            pos.x = nx;
                            pos.y = ny;
                        }
                    }
                }

                // Clean up old notifications
                self.notifications.retain(|(t, _)| self.tick - t < 200);

                // Tick down milestone banner
                if let Some(ref mut banner) = self.milestone_banner {
                    banner.ticks_remaining = banner.ticks_remaining.saturating_sub(1);
                    if banner.ticks_remaining == 0 {
                        self.milestone_banner = None;
                    }
                }

                // Update event system
                self.update_events();
                self.check_milestones();

                // Plague: kill a villager every 100 game ticks while plague is active
                if self.tick.is_multiple_of(100) {
                    let mut should_kill = false;
                    for event in &mut self.events.active_events {
                        if let GameEvent::Plague {
                            kills_remaining, ..
                        } = event
                            && *kills_remaining > 0
                        {
                            *kills_remaining -= 1;
                            should_kill = true;
                            break;
                        }
                    }
                    if should_kill {
                        let victim: Option<hecs::Entity> = self
                            .world
                            .query::<(hecs::Entity, &Creature)>()
                            .iter()
                            .find(|(_, c)| c.species == Species::Villager)
                            .map(|(e, _)| e);
                        if let Some(entity) = victim {
                            let _ = self.world.despawn(entity);
                            self.notify("A villager succumbed to plague!".to_string());
                        }
                    }
                }

                // Apply seasonal modifiers
                let mods = self.day_night.season_modifiers();

                ecs::system_hunger(&mut self.world, mods.hunger_mult, self.tick);

                // Siege model: compute if settlement is defended
                let defense_rating = self.compute_defense_rating();
                let (scx, scy) = self.settlement_center();
                let wolves_near_count = self
                    .world
                    .query::<(&Position, &Creature)>()
                    .iter()
                    .filter(|(_, c)| c.species == Species::Predator)
                    .filter(|(p, _)| {
                        let dx = p.x - scx as f64;
                        let dy = p.y - scy as f64;
                        dx * dx + dy * dy < 900.0 // within ~30 tiles
                    })
                    .count();
                let attack_str = wolves_near_count as f64 * 0.5;
                let settlement_defended = defense_rating > attack_str;
                if wolves_near_count > 0 && settlement_defended {
                    // Only notify once per ~200 ticks to avoid spam
                    if self.tick.is_multiple_of(200) {
                        self.notify("Wolf pack repelled by defenses!".to_string());
                    }
                    self.skills.military += 0.002 * wolves_near_count as f64;
                }

                // Seasonal gathering multiplier: spring 1.1x, summer 1.0x, winter 0.6x.
                // Autumn wood bonus stacks on top (1.5x from #36).
                let seasonal_gather = mods.gathering_mult;
                let autumn_wood_bonus = if self.day_night.season == Season::Autumn {
                    1.5
                } else {
                    1.0
                };
                let skill_mults = SkillMults {
                    gather_wood_speed: (1.0 + self.skills.woodcutting / 50.0)
                        * autumn_wood_bonus
                        * seasonal_gather,
                    gather_stone_speed: (1.0 + self.skills.mining / 50.0) * seasonal_gather,
                    build_speed: (self.skills.building / 50.0).floor() as u32,
                };
                self.spatial_grid.populate(&self.world);
                // Group detection: rebuild groups periodically, check threats every tick
                if self
                    .tick
                    .saturating_sub(self.group_manager.last_detection_tick)
                    >= crate::ecs::groups::GROUP_DETECTION_INTERVAL
                {
                    self.group_manager
                        .detect_groups(&self.world, &self.spatial_grid, self.tick);
                }
                self.group_manager.update_threat_detection(
                    &mut self.world,
                    &self.spatial_grid,
                    self.tick,
                );
                self.ai_arrays.extract(&self.world);
                ecs::system_update_memories(
                    &mut self.world,
                    &self.map,
                    &self.spatial_grid,
                    self.tick,
                );
                ecs::system_info_sharing(&mut self.world, &self.spatial_grid, self.tick);
                let ai_result = ecs::system_ai(
                    &mut self.world,
                    &self.map,
                    &self.spatial_grid,
                    mods.wolf_aggression,
                    self.resources.food,
                    self.resources.wood,
                    self.resources.stone,
                    self.resources.grain,
                    self.resources.bread,
                    &skill_mults,
                    settlement_defended,
                    self.day_night.is_night(),
                    &self.knowledge.frontier,
                    self.tick,
                    &self.fire_tiles,
                    &self.danger_scent,
                    &self.home_scent,
                    &self.nav_graph,
                    &self.group_manager,
                    &self.flow_fields,
                );
                let mut deposited_food = 0u32;
                let mut deposited_wood = 0u32;
                let mut deposited_stone = 0u32;
                for res in ai_result.deposited {
                    match res {
                        ResourceType::Food => {
                            self.resources.food += 1;
                            deposited_food += 1;
                        }
                        ResourceType::Wood => {
                            self.resources.wood += 1;
                            deposited_wood += 1;
                        }
                        ResourceType::Stone => {
                            self.resources.stone += 1;
                            deposited_stone += 1;
                        }
                        _ => {} // Refined resources (Planks, Masonry, Grain) not gathered by villagers
                    }
                }
                if deposited_food > 0 {
                    self.notify(format!("Resource deposited: +{} food", deposited_food));
                }
                if deposited_wood > 0 {
                    self.notify(format!("Resource deposited: +{} wood", deposited_wood));
                }
                if deposited_stone > 0 {
                    self.notify(format!("Resource deposited: +{} stone", deposited_stone));
                }
                // Deforestation: convert Forest tiles to Stump where wood was harvested
                // Also degrade fertility at the deforested tile and its 4-neighbors.
                for (hx, hy) in &ai_result.wood_harvest_positions {
                    let ix = hx.round() as usize;
                    let iy = hy.round() as usize;
                    if self.map.get(ix, iy) == Some(&Terrain::Forest) {
                        self.map.set(ix, iy, Terrain::Stump);
                        self.dirty.mark(ix, iy);
                        self.nav_graph.mark_dirty(ix, iy);
                        // Deforestation erosion: exposed soil loses fertility
                        self.soil_fertility.degrade(ix, iy, 0.05);
                        for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                            let nx = ix as i32 + dx;
                            let ny = iy as i32 + dy;
                            if nx >= 0 && ny >= 0 {
                                self.soil_fertility.degrade(nx as usize, ny as usize, 0.05);
                            }
                        }
                    }
                }
                // Mining terrain changes: Mountain -> Quarry -> QuarryDeep
                // Mining scars soil fertility at the mined tile and its neighbors.
                for (hx, hy) in &ai_result.stone_harvest_positions {
                    let vx = hx.round() as i32;
                    let vy = hy.round() as i32;
                    // Find the Mountain/Quarry tile adjacent to the villager's position
                    for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                        let mx = vx + dx;
                        let my = vy + dy;
                        if mx >= 0 && my >= 0 {
                            let ux = mx as usize;
                            let uy = my as usize;
                            match self.map.get(ux, uy) {
                                Some(&Terrain::Mountain) | Some(&Terrain::Quarry) => {
                                    let prev_terrain = *self.map.get(ux, uy).unwrap();
                                    let count = self.map.increment_mine_count(ux, uy);
                                    if count >= 12 {
                                        self.map.set(ux, uy, Terrain::QuarryDeep);
                                        self.dirty.mark(ux, uy);
                                        self.nav_graph.mark_dirty(ux, uy);
                                        // QuarryDeep: set fertility to 0.05, neighbors lose 0.1
                                        self.soil_fertility.set(ux, uy, 0.05);
                                        for &(ndx, ndy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)]
                                        {
                                            let nx = mx + ndx;
                                            let ny = my + ndy;
                                            if nx >= 0 && ny >= 0 {
                                                self.soil_fertility.degrade(
                                                    nx as usize,
                                                    ny as usize,
                                                    0.1,
                                                );
                                            }
                                        }
                                    } else if count >= 6 && prev_terrain == Terrain::Mountain {
                                        self.map.set(ux, uy, Terrain::Quarry);
                                        self.dirty.mark(ux, uy);
                                        self.nav_graph.mark_dirty(ux, uy);
                                        // Quarry: set fertility to 0.05, neighbors lose 0.1
                                        self.soil_fertility.set(ux, uy, 0.05);
                                        for &(ndx, ndy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)]
                                        {
                                            let nx = mx + ndx;
                                            let ny = my + ndy;
                                            if nx >= 0 && ny >= 0 {
                                                self.soil_fertility.degrade(
                                                    nx as usize,
                                                    ny as usize,
                                                    0.1,
                                                );
                                            }
                                        }
                                    }
                                    break; // only affect one tile per harvest
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // Stone deposit depletion: set ScarredGround where deposits were fully mined
                for (sx, sy) in &ai_result.depleted_stone_positions {
                    let ix = sx.round() as usize;
                    let iy = sy.round() as usize;
                    self.map.set(ix, iy, Terrain::ScarredGround);
                    self.dirty.mark(ix, iy);
                }
                if ai_result.grain_consumed > 0 {
                    self.resources.grain = self
                        .resources
                        .grain
                        .saturating_sub(ai_result.grain_consumed);
                    self.notify(format!(
                        "Villager ate grain (-{})",
                        ai_result.grain_consumed
                    ));
                }
                if ai_result.bread_consumed > 0 {
                    self.resources.bread = self
                        .resources
                        .bread
                        .saturating_sub(ai_result.bread_consumed);
                    self.notify(format!(
                        "Villager ate bread (-{})",
                        ai_result.bread_consumed
                    ));
                }
                if ai_result.food_consumed > 0 {
                    self.resources.food =
                        self.resources.food.saturating_sub(ai_result.food_consumed);
                    self.notify(format!(
                        "Villager ate from stockpile (-{} food)",
                        ai_result.food_consumed
                    ));
                }

                // Skill gains from activity
                let skill_gain = 0.01;
                self.skills.woodcutting += ai_result.woodcutting_ticks as f64 * skill_gain;
                self.skills.mining += ai_result.mining_ticks as f64 * skill_gain;
                self.skills.farming += ai_result.farming_ticks as f64 * skill_gain;
                self.skills.building += ai_result.building_ticks as f64 * skill_gain;

                // Apply flow field demand requests from AI
                for (dx, dy) in ai_result.flow_field_requests {
                    self.flow_fields.request(dx, dy);
                }

                // Skill decay (slow loss when inactive)
                let decay = 0.9999;
                self.skills.farming *= decay;
                self.skills.mining *= decay;
                self.skills.woodcutting *= decay;
                self.skills.building *= decay;
                self.skills.military *= decay;

                // Clamp skills to [0, 100]
                self.skills.farming = self.skills.farming.clamp(0.0, 100.0);
                self.skills.mining = self.skills.mining.clamp(0.0, 100.0);
                self.skills.woodcutting = self.skills.woodcutting.clamp(0.0, 100.0);
                self.skills.building = self.skills.building.clamp(0.0, 100.0);
                self.skills.military = self.skills.military.clamp(0.0, 100.0);

                // Snapshot positions before movement for dirty-rect marking
                let pre_positions: Vec<(hecs::Entity, usize, usize)> = self
                    .world
                    .query::<(hecs::Entity, &Position)>()
                    .iter()
                    .map(|(e, p)| (e, p.x.round() as usize, p.y.round() as usize))
                    .collect();

                ecs::system_movement(&mut self.world, &self.map);

                // Mark old + new positions dirty for any entity that moved
                for (entity, old_x, old_y) in &pre_positions {
                    if let Ok(pos) = self.world.get::<&Position>(*entity) {
                        let new_x = pos.x.round() as usize;
                        let new_y = pos.y.round() as usize;
                        if new_x != *old_x || new_y != *old_y {
                            self.dirty.mark(*old_x, *old_y);
                            self.dirty.mark(new_x, new_y);
                        }
                    }
                }

                // Update exploration: only villagers reveal fog of war
                for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
                    if creature.species != Species::Villager {
                        continue;
                    }
                    let x = pos.x as usize;
                    let y = pos.y as usize;
                    self.exploration.reveal(x, y, creature.sight_range as usize);
                }

                // Count creatures before breeding to detect new spawns
                let prey_before = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Prey)
                    .count();
                let wolf_before = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Predator)
                    .count();

                let wolf_boost = self.events.wolf_spawn_multiplier();
                ecs::system_breeding(
                    &mut self.world,
                    self.day_night.season,
                    wolf_boost,
                    self.day_night.year,
                );

                // Coordinated wolf raids
                let (scx, scy) = self.settlement_center();
                if ecs::system_wolf_raids(
                    &mut self.world,
                    scx as f64,
                    scy as f64,
                    self.tick,
                    self.day_night.year,
                ) {
                    self.notify("Wolf pack is raiding the settlement!".to_string());
                }

                let prey_after = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Prey)
                    .count();
                let wolf_after = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Predator)
                    .count();
                if prey_after > prey_before {
                    self.notify("New rabbit born!".to_string());
                }
                if wolf_after > wolf_before {
                    self.notify("New wolf born!".to_string());
                }

                // Count species before death to detect who died
                let villagers_before = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Villager)
                    .count();
                let prey_before_death = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Prey)
                    .count();
                let wolves_before_death = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Predator)
                    .count();

                let dead = ecs::system_death(&mut self.world);
                self.group_manager
                    .remove_dead_entities(&dead, &mut self.world, self.tick);

                let villagers_after = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Villager)
                    .count();
                let prey_after_death = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Prey)
                    .count();
                let wolves_after_death = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Predator)
                    .count();

                let villager_deaths = villagers_before.saturating_sub(villagers_after);
                let prey_deaths = prey_before_death.saturating_sub(prey_after_death);
                let wolf_deaths = wolves_before_death.saturating_sub(wolves_after_death);
                if villager_deaths > 0 {
                    self.notify("Villager died!".to_string());
                }
                if prey_deaths > 0 {
                    self.notify("A rabbit was killed!".to_string());
                }
                if wolf_deaths > 0 {
                    self.notify("A wolf died!".to_string());
                }

                // Track peak population and detect game over
                let villager_count = villagers_after as u32;
                if villager_count > self.peak_population {
                    self.peak_population = villager_count;
                }
                if villager_count == 0 && villagers_before > 0 {
                    self.game_over = true;
                    self.paused = true;
                    self.notify("All villagers have perished!".to_string());
                }

                // Farm growth (only advances when villager is present at farm)
                // Drought/BountifulHarvest no longer multiply yield directly --
                // they modify rain_rate, which flows through water -> moisture -> growth.
                let farm_mult = 1.0 + self.skills.farming / 100.0;
                ecs::system_farms(
                    &mut self.world,
                    self.day_night.season,
                    farm_mult,
                    &self.moisture,
                    &mut self.soil_fertility,
                    &self.soil,
                );

                // Assign idle villagers to farms/workshops, then mark worker presence
                ecs::system_assign_workers(&mut self.world, &self.resources);
                let farm_food_picked = ecs::system_mark_workers(&mut self.world);
                self.resources.food += farm_food_picked;
                if farm_food_picked > 0 {
                    self.notify(format!(
                        "Farm harvest collected: +{} food",
                        farm_food_picked
                    ));
                }

                // Active farms with workers contribute to farming skill
                let tended_farms = self
                    .world
                    .query::<&FarmPlot>()
                    .iter()
                    .filter(|f| f.worker_present)
                    .count() as f64;
                self.skills.farming += tended_farms * 0.003;

                // Processing buildings (only advance when villager is present)
                let process_mult = 1.0;
                ecs::system_processing(&mut self.world, &mut self.resources, process_mult);

                // Update particles: move, age, and remove dead ones.
                self.update_particles();

                // Wind flow particles + activity particles
                self.spawn_wind_particles();
                self.spawn_activity_particles();

                // Winter food decay: percentage-based spoilage, grain is preserved
                if self.day_night.season == Season::Winter
                    && self.tick.is_multiple_of(30)
                    && self.resources.food > 0
                {
                    // Cap decay at 2 per event so large stockpiles don't evaporate over a winter.
                    // Granary (converts food→grain) prevents spoilage entirely.
                    let decay = std::cmp::min(2, std::cmp::max(1, self.resources.food * 2 / 100));
                    self.resources.food = self.resources.food.saturating_sub(decay);
                    self.notify(format!("Food spoiled in winter (-{})", decay));
                }

                // Resource regrowth (deforestation lifecycle: Stump -> Bare -> Sapling -> Forest)
                ecs::system_regrowth(&mut self.world, &mut self.map, &self.vegetation, self.tick);

                // Forest fire system: check ignition ~once per in-game day (1200 ticks)
                if self.tick.is_multiple_of(1200) {
                    self.check_fire_ignition();
                }
                // Process fire spread and burnout every tick
                self.tick_fire();

                // Soil fertility recovery (every 50 ticks)
                if self.tick.is_multiple_of(50) {
                    ecs::system_soil_recovery(
                        &self.world,
                        &mut self.soil_fertility,
                        &self.soil,
                        &self.vegetation,
                        &self.moisture,
                        self.day_night.season,
                        &self.map,
                    );
                }

                // Reclassify biomes based on current conditions (every 500 ticks)
                if self.tick.is_multiple_of(500) {
                    self.reclassify_biomes();
                }

                // Check for completed buildings
                self.check_build_completion();

                // Update influence map: villagers emit 1.0, active build sites emit 0.5
                self.update_influence();

                // Track villager foot traffic and auto-build roads
                self.update_traffic();

                // Update environmental scent traces (danger, home)
                self.update_traces();

                // Population growth check
                self.try_population_growth();

                // Recompute chokepoint map if terrain changed (building placed/demolished).
                if self.chokepoints_dirty && self.tick.is_multiple_of(50) {
                    self.chokepoint_map =
                        chokepoint::ChokepointMap::compute(&self.map, &self.river_mask);
                    self.chokepoints_dirty = false;
                }

                // If terrain changed this tick (nav graph has dirty regions), mark
                // flow fields for invalidation so they recompute on next maintain.
                if !self.nav_graph.dirty_regions.is_empty() {
                    self.terrain_dirty_tick = self.tick;
                    self.flow_fields.mark_terrain_dirty(self.tick);
                }

                // Process dirty navigation regions (hierarchical pathfinding incremental update).
                // Capped at 8 regions per tick to bound per-frame cost.
                self.nav_graph.process_dirty(&self.map);

                // Stockpile always-on flow field: ensure stockpile destinations
                // always have demand so their flow fields persist.
                for sp in self
                    .spatial_grid
                    .all_of_category(crate::ecs::spatial::category::STOCKPILE)
                {
                    let key = (sp.x.round() as usize, sp.y.round() as usize);
                    // Inject enough demand to keep the field alive
                    for _ in 0..crate::pathfinding::flow_field::FLOW_FIELD_THRESHOLD {
                        self.flow_fields.request(key.0, key.1);
                    }
                }

                // Maintain flow fields: create/recompute/evict based on demand.
                // At most 2 computes per tick (~2ms budget).
                self.flow_fields.maintain(&self.map, self.tick);

                // Recompute threat map every 100 ticks (wolf territory, garrison coverage,
                // corridor pressure, and exposure gaps for the Threats overlay).
                if self.tick.is_multiple_of(100) {
                    self.update_threat_map();
                }

                // Auto-build check (every 50 ticks — frequent enough to catch narrow
                // resource windows, e.g. wood=8-9 where Workshop is affordable but Hut is not)
                if self.auto_build && self.tick.is_multiple_of(50) {
                    self.auto_build_tick();
                }

                // Outpost lifecycle: update activity and abandon depleted outposts.
                if self.tick.is_multiple_of(100) {
                    self.update_outposts();
                }

                // Update settlement knowledge (frontier, known resources) for exploration AI.
                if self.tick.is_multiple_of(100) {
                    self.update_settlement_knowledge();
                }

                // Seasonal config for rain/water — events chain through here
                let mut tick_config = self.sim_config.clone();
                tick_config.rain_rate *= mods.rain_mult;
                tick_config.evaporation *= mods.evap_mult;

                // Drought reduces rain by 60% (chain: less rain -> less water -> less
                // moisture -> slower farm growth). No direct yield multiplier needed.
                if self.events.has_drought() {
                    tick_config.rain_rate *= 0.4;
                }
                // Bountiful harvest increases rain by 50% (chain: more rain -> more
                // water -> more moisture -> faster farm growth).
                if self.events.has_bountiful_harvest() {
                    tick_config.rain_rate *= 1.5;
                }

                // Seasonal auto-rain: light rain every 200 ticks in wet seasons + manual toggle
                // No auto-rain — game starts dry. Player toggles rain with 'r'.
                // Rain will come naturally from wind-moisture cycle over water bodies.
                let should_rain = self.raining;

                // Water cycle: moisture, wind advection, pipe water, sediment, vegetation, erosion
                self.step_water_cycle(should_rain, mods.veg_growth_mult);

                // advance day/night cycle and compute Blinn-Phong lighting + shadows (viewport only)
                let prev_season = self.day_night.season;
                self.day_night.tick();
                if self.day_night.season != prev_season {
                    self.handle_season_change(prev_season);
                }

                // Lua scripting hooks
                #[cfg(feature = "lua")]
                {
                    self.lua_hot_reload();
                    self.lua_on_tick();
                }
            }
            if self.day_night.enabled {
                self.day_night.compute_lighting(
                    &self.heights,
                    self.map.width,
                    self.map.height,
                    self.camera.x,
                    self.camera.y,
                    world_vw as usize,
                    vh as usize,
                );
            } // end speed loop
        } // end if !self.paused

        // Dirty-rect: detect camera scroll → mark everything dirty
        if self.camera.x != self.prev_camera_x || self.camera.y != self.prev_camera_y {
            self.dirty.mark_all();
            self.prev_camera_x = self.camera.x;
            self.prev_camera_y = self.camera.y;
        }

        // Weather particles change position every tick: mark all dirty when
        // weather effects are visible to avoid stale rain/snow artifacts.
        {
            let has_blizzard = self
                .events
                .active_events
                .iter()
                .any(|e| matches!(e, GameEvent::Blizzard { .. }));
            let weather_visible = (self.raining || has_blizzard)
                && !matches!(self.render_mode, RenderMode::Map | RenderMode::Debug);
            if weather_visible {
                self.dirty.mark_all();
            }

            // Normal and Landscape modes use dynamic lighting (sun/moon position changes
            // every tick), so all visible tiles need redrawing. Only Map mode (no lighting)
            // benefits from dirty-rect skipping.
            if matches!(self.render_mode, RenderMode::Normal | RenderMode::Landscape) {
                self.dirty.mark_all();
            }
        }

        // render — skip renderer.clear() so the front buffer retains previous
        // frame content for clean tiles (dirty-rect optimization).
        match self.render_mode {
            RenderMode::Debug => self.draw_debug(renderer),
            RenderMode::Map => self.draw_map_mode(renderer),
            RenderMode::Landscape => self.draw_landscape_mode(renderer),
            RenderMode::Normal => self.draw(renderer),
        }
        if self.game_over {
            self.draw_game_over(renderer);
        }
        renderer.flush()?;

        // Clear dirty bits after render+flush so next frame starts clean
        self.dirty.clear();

        Ok(())
    }

    pub fn step_headless(
        &mut self,
        input: GameInput,
        renderer: &mut HeadlessRenderer,
    ) -> Result<FrameSnapshot> {
        self.step(input, renderer)?;
        Ok(self.snapshot(renderer))
    }

    pub fn run_script(
        &mut self,
        inputs: &[GameInput],
        renderer: &mut HeadlessRenderer,
    ) -> Result<Vec<FrameSnapshot>> {
        let mut snapshots = Vec::with_capacity(inputs.len());
        for &input in inputs {
            snapshots.push(self.step_headless(input, renderer)?);
        }
        Ok(snapshots)
    }

    /// Collect structured diagnostics data as a JSON-serializable map.
    pub fn collect_diagnostics(&self) -> serde_json::Value {
        use std::collections::BTreeMap;

        // Population count
        let mut villager_count = 0u32;
        let mut wolf_count = 0u32;
        let mut rabbit_count = 0u32;
        let mut villager_states: BTreeMap<String, u32> = BTreeMap::new();

        for creature in self.world.query::<(&Creature, &Behavior)>().iter() {
            let (c, b) = creature;
            match c.species {
                Species::Villager => {
                    villager_count += 1;
                    let state_name = match &b.state {
                        BehaviorState::Wander { .. } => "Wander",
                        BehaviorState::Seek { .. } => "Seek",
                        BehaviorState::Idle { .. } => "Idle",
                        BehaviorState::Eating { .. } => "Eating",
                        BehaviorState::FleeHome { .. } => "FleeHome",
                        BehaviorState::AtHome { .. } => "AtHome",
                        BehaviorState::Hunting { .. } => "Hunting",
                        BehaviorState::Captured => "Captured",
                        BehaviorState::Gathering { .. } => "Gathering",
                        BehaviorState::Hauling { .. } => "Hauling",
                        BehaviorState::Sleeping { .. } => "Sleeping",
                        BehaviorState::Building { .. } => "Building",
                        BehaviorState::Exploring { .. } => "Exploring",
                        BehaviorState::Farming { .. } => "Farming",
                        BehaviorState::Working { .. } => "Working",
                    };
                    *villager_states.entry(state_name.to_string()).or_insert(0) += 1;
                }
                Species::Predator => wolf_count += 1,
                Species::Prey => rabbit_count += 1,
            }
        }

        // Building counts — count completed building marker components
        let mut building_counts: BTreeMap<String, u32> = BTreeMap::new();
        let hut_count = self.world.query::<&HutBuilding>().iter().count() as u32;
        let garrison_count = self.world.query::<&GarrisonBuilding>().iter().count() as u32;
        let farm_count = self.world.query::<&FarmPlot>().iter().count() as u32;
        let stockpile_count = self.world.query::<&Stockpile>().iter().count() as u32;
        let workshop_count = self.world.query::<&ProcessingBuilding>().iter().count() as u32;
        if hut_count > 0 {
            building_counts.insert("Hut".to_string(), hut_count);
        }
        if garrison_count > 0 {
            building_counts.insert("Garrison".to_string(), garrison_count);
        }
        if farm_count > 0 {
            building_counts.insert("Farm".to_string(), farm_count);
        }
        if stockpile_count > 0 {
            building_counts.insert("Stockpile".to_string(), stockpile_count);
        }
        if workshop_count > 0 {
            building_counts.insert("Workshop".to_string(), workshop_count);
        }
        let build_site_count = self.world.query::<&BuildSite>().iter().count() as u32;

        // Events
        let event_names: Vec<String> = self
            .events
            .active_events
            .iter()
            .map(|e| match e {
                GameEvent::Drought { .. } => "Drought".to_string(),
                GameEvent::BountifulHarvest { .. } => "BountifulHarvest".to_string(),
                GameEvent::Migration { .. } => "Migration".to_string(),
                GameEvent::WolfSurge { .. } => "WolfSurge".to_string(),
                GameEvent::Plague { .. } => "Plague".to_string(),
                GameEvent::Blizzard { .. } => "Blizzard".to_string(),
                GameEvent::BanditRaid { .. } => "BanditRaid".to_string(),
            })
            .collect();

        // Terrain summary
        let map_w = self.map.width;
        let map_h = self.map.height;
        let total_tiles = (map_w * map_h) as f64;
        let mut biome_counts: BTreeMap<String, u32> = BTreeMap::new();
        let mut height_sum = 0.0_f64;
        let mut moisture_sum = 0.0_f64;
        let mut vegetation_sum = 0.0_f64;
        let mut water_tiles = 0u32;
        for y in 0..map_h {
            for x in 0..map_w {
                if let Some(t) = self.map.get(x, y) {
                    let name = format!("{:?}", t);
                    *biome_counts.entry(name).or_insert(0) += 1;
                    if matches!(t, Terrain::Water) {
                        water_tiles += 1;
                    }
                }
                let idx = y * map_w + x;
                if idx < self.heights.len() {
                    height_sum += self.heights[idx];
                }
                moisture_sum += self.moisture.get(x, y);
                vegetation_sum += self.vegetation.get(x, y);
            }
        }
        let biome_distribution: BTreeMap<String, f64> = biome_counts
            .iter()
            .map(|(k, v)| (k.clone(), (*v as f64 / total_tiles * 1000.0).round() / 10.0))
            .collect();
        let round1 = |v: f64| (v * 100.0).round() / 100.0;
        let avg_height = round1(height_sum / total_tiles);
        let avg_moisture = round1(moisture_sum / total_tiles);
        let avg_vegetation = round1(vegetation_sum / total_tiles);
        let water_coverage_pct = (water_tiles as f64 / total_tiles * 1000.0).round() / 10.0;
        let pipe_water_total = round1(self.pipe_water.total_water());
        let wind_moisture_total = round1(self.wind.moisture_carried.iter().copied().sum::<f64>());

        // Settlement metrics (derived from influence map + exploration)
        let mut footprint_tiles = 0u32;
        for y in 0..map_h {
            for x in 0..map_w {
                if self.influence.get(x, y) > 0.1 {
                    footprint_tiles += 1;
                }
            }
        }
        let explored = self.exploration.revealed.iter().filter(|&&r| r).count();
        let exploration_pct = round1(explored as f64 / total_tiles * 100.0);
        let outpost_count = self.outposts.len() as u32;

        // Housing metrics
        let mut hut_capacity = 0u32;
        for h in self.world.query::<&HutBuilding>().iter() {
            hut_capacity += h.capacity;
        }

        // Threat metrics
        let fire_tile_count = self.fire_tiles.len() as u32;
        let active_event_count = self.events.active_events.len() as u32;

        // Terrain extension: elevation std + slope distribution
        let height_mean = height_sum / total_tiles;
        let height_var_sum: f64 = self
            .heights
            .iter()
            .map(|h| (h - height_mean).powi(2))
            .sum();
        let elevation_std = round1((height_var_sum / total_tiles).sqrt());

        let (mut slope_flat, mut slope_gentle, mut slope_steep, mut slope_cliff) =
            (0u32, 0u32, 0u32, 0u32);
        for &s in &self.pipeline_slope {
            if s < 0.05 {
                slope_flat += 1;
            } else if s < 0.15 {
                slope_gentle += 1;
            } else if s < 0.3 {
                slope_steep += 1;
            } else {
                slope_cliff += 1;
            }
        }

        serde_json::json!({
            "tick": self.tick,
            "seed": self.terrain_config.seed,
            "population": villager_count,
            "resources": {
                "food": self.resources.food,
                "wood": self.resources.wood,
                "stone": self.resources.stone,
                "planks": self.resources.planks,
                "masonry": self.resources.masonry,
                "grain": self.resources.grain,
                "bread": self.resources.bread,
            },
            "villager_states": villager_states,
            "buildings": building_counts,
            "build_sites": build_site_count,
            "season": self.day_night.season.name(),
            "year": self.day_night.year + 1,
            "day_night": if self.day_night.is_night() { "night" } else { "day" },
            "events": event_names,
            "wolves": wolf_count,
            "rabbits": rabbit_count,
            "skills": {
                "farm": self.skills.farming,
                "mine": self.skills.mining,
                "wood": self.skills.woodcutting,
                "build": self.skills.building,
            },
            "terrain": {
                "biome_distribution": biome_distribution,
                "avg_height": avg_height,
                "avg_moisture": avg_moisture,
                "avg_vegetation": avg_vegetation,
                "water_coverage_pct": water_coverage_pct,
                "pipe_water_total": pipe_water_total,
                "wind_moisture_total": wind_moisture_total,
                "elevation_std": elevation_std,
                "slope_distribution": {
                    "flat_pct": round1(slope_flat as f64 / total_tiles * 100.0),
                    "gentle_pct": round1(slope_gentle as f64 / total_tiles * 100.0),
                    "steep_pct": round1(slope_steep as f64 / total_tiles * 100.0),
                    "cliff_pct": round1(slope_cliff as f64 / total_tiles * 100.0),
                },
            },
            "settlement": {
                "footprint_tiles": footprint_tiles,
                "exploration_pct": exploration_pct,
                "outpost_count": outpost_count,
            },
            "housing": {
                "hut_capacity": hut_capacity,
                "population": villager_count,
                "growth_potential": hut_capacity.saturating_sub(villager_count),
            },
            "threats": {
                "fire_tiles": fire_tile_count,
                "active_events": active_event_count,
                "threat_score": round1(self.threat_score),
            },
        })
    }

    fn snapshot(&self, renderer: &HeadlessRenderer) -> FrameSnapshot {
        let (w, h) = renderer.size();
        let mut cells = Vec::with_capacity(h as usize);
        for y in 0..h {
            let mut row = Vec::with_capacity(w as usize);
            for x in 0..w {
                row.push(*renderer.get_cell(x, y).unwrap());
            }
            cells.push(row);
        }
        FrameSnapshot {
            tick: self.tick,
            width: w,
            height: h,
            text: renderer.frame_as_string(),
            cells,
        }
    }
}
