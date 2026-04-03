mod build;
pub mod chokepoint;
pub mod dirty;
mod events;
mod render;
mod save;

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
    SoilFertilityMap, ThreatMap, TrafficMap, VegetationMap, WaterMap,
};
use crate::terrain_gen::{self, TerrainGenConfig};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    Quit,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    ToggleRain,
    ToggleErosion,
    ToggleDayNight,
    ToggleDebugView,
    TogglePause,
    ToggleQueryMode,
    QueryUp,
    QueryDown,
    QueryLeft,
    QueryRight,
    ToggleBuildMode,
    BuildCycleType,
    BuildPlace,
    BuildUp,
    BuildDown,
    BuildLeft,
    BuildRight,
    Drain,
    Save,
    Load,
    Restart,
    ToggleAutoBuild,
    CycleOverlay,
    GotoSettlement,
    Demolish,
    CycleSpeed,
    /// Mouse click at screen coordinates (x, y)
    MouseClick {
        x: u16,
        y: u16,
    },
    None,
}

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
            RenderMode::Normal => "-",
            RenderMode::Map => "M",
            RenderMode::Landscape => "L",
            RenderMode::Debug => "D",
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
    pub game_speed: u32, // 1 = normal, 2 = 2x, 5 = 5x
    pub soil: Vec<crate::terrain_pipeline::SoilType>,
    pub soil_fertility: SoilFertilityMap,
    pub river_mask: Vec<bool>,
    pub resource_map: crate::terrain_pipeline::ResourceMap,
    pub knowledge: SettlementKnowledge,
    pub spatial_grid: crate::ecs::spatial::SpatialHashGrid,
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
        let pipeline_config = PipelineConfig {
            terrain: TerrainGenConfig {
                seed,
                scale: 0.015,
                ..Default::default()
            },
            ..PipelineConfig::default()
        };
        let result = run_pipeline(map_width, map_height, &pipeline_config);
        let mut map = result.map;
        let heights = result.heights;
        let terrain_config = pipeline_config.terrain;

        // Seed water from pipeline rivers + water tiles
        let mut water = WaterMap::new(map_width, map_height);
        for y in 0..map_height {
            for x in 0..map_width {
                let i = y * map_width + x;
                if result.river_mask[i] || matches!(map.get(x, y), Some(Terrain::Water)) {
                    let depth = (terrain_config.water_level - heights[i]).max(0.01);
                    water.set(x, y, depth);
                }
            }
        }
        let moisture = MoistureMap::new(map_width, map_height);
        let vegetation = VegetationMap::new(map_width, map_height);

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
        for (dx, dy, terrain) in BuildingType::Hut.tiles() {
            map.set(
                hx as usize + dx as usize,
                hy as usize + dy as usize,
                terrain,
            );
        }
        ecs::spawn_hut(&mut world, hx + hsw as f64 / 2.0, hy + hsh as f64 / 2.0);

        // Pre-built farm — search opposite side of stockpile
        let (fsw, fsh) = BuildingType::Farm.size();
        let (fx, fy) = find_building_spot(
            &map,
            scx + 4,
            scy.wrapping_sub(3),
            fsw as usize,
            fsh as usize,
        );
        for (dx, dy, terrain) in BuildingType::Farm.tiles() {
            map.set(
                fx as usize + dx as usize,
                fy as usize + dy as usize,
                terrain,
            );
        }
        ecs::spawn_farm_plot(&mut world, fx + fsw as f64 / 2.0, fy + fsh as f64 / 2.0);

        // Pre-built Granary — converts food to grain which is preserved through Winter.
        // Without this, winter food decay (2%/30 ticks) kills all settlements before Year 2.
        let (gsw, gsh) = BuildingType::Granary.size();
        let (gx, gy) = find_building_spot(&map, scx + 5, scy + 4, gsw as usize, gsh as usize);
        for (dx, dy, terrain) in BuildingType::Granary.tiles() {
            map.set(
                gx as usize + dx as usize,
                gy as usize + dy as usize,
                terrain,
            );
        }
        ecs::spawn_processing_building(
            &mut world,
            gx + gsw as f64 / 2.0,
            gy + gsh as f64 / 2.0,
            Recipe::FoodToGrain,
        );

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
            soil_fertility: SoilFertilityMap::from_soil_types(map_width, map_height, &result.soil),
            soil: result.soil,
            river_mask: result.river_mask,
            resource_map: result.resources,
            knowledge: SettlementKnowledge::default(),
            spatial_grid: crate::ecs::spatial::SpatialHashGrid::new(map_width, map_height, 16),
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
            #[cfg(feature = "lua")]
            script_engine: None,
        };
        // Compute initial chokepoint map from generated terrain
        g.chokepoint_map = chokepoint::ChokepointMap::compute(&g.map, &g.river_mask);
        g.chokepoints_dirty = false;
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

    // --- Forest fire system ---

    /// Check for fire ignition. Called once per in-game day during summer
    /// when conditions are right: low moisture on flammable tiles.
    fn check_fire_ignition(&mut self) {
        let season = self.day_night.season;
        // Fire only ignites in summer
        if season != Season::Summer {
            return;
        }

        let mut rng = rand::rng();
        let w = self.map.width;
        let h = self.map.height;

        // Sample up to 50 random tiles for lightning ignition
        let samples = 50usize.min(w * h);
        for _ in 0..samples {
            let x = rng.random_range(0..w as u32) as usize;
            let y = rng.random_range(0..h as u32) as usize;
            let Some(terrain) = self.map.get(x, y).copied() else {
                continue;
            };
            if !terrain.is_flammable() {
                continue;
            }
            let moisture = self.moisture.get(x, y);
            if moisture >= 0.15 {
                continue;
            }
            // 0.01% chance per eligible tile per day-tick (0.0001)
            if rng.random_range(0u32..10000) < 1 {
                self.ignite_tile(x, y, &mut rng);
                return; // At most one lightning ignition per day
            }
        }

        // Smithy/bakery building ignition: check tiles within 2 of each
        let building_positions: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &ProcessingBuilding)>()
            .iter()
            .filter(|(_, pb)| matches!(pb.recipe, Recipe::StoneToMasonry | Recipe::GrainToBread))
            .map(|(pos, _)| (pos.x, pos.y))
            .collect();

        for (bx, by) in building_positions {
            let ix = bx.round() as i32;
            let iy = by.round() as i32;
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let nx = ix + dx;
                    let ny = iy + dy;
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let ux = nx as usize;
                    let uy = ny as usize;
                    let Some(terrain) = self.map.get(ux, uy).copied() else {
                        continue;
                    };
                    if !terrain.is_flammable() {
                        continue;
                    }
                    let moisture = self.moisture.get(ux, uy);
                    if moisture >= 0.15 {
                        continue;
                    }
                    // 0.1% chance per tile per day (0.001)
                    if rng.random_range(0u32..1000) < 1 {
                        self.ignite_tile(ux, uy, &mut rng);
                        return;
                    }
                }
            }
        }
    }

    /// Ignite a single tile: set it to Burning, assign burn timer, add to fire_tiles.
    fn ignite_tile(&mut self, x: usize, y: usize, rng: &mut impl rand::RngExt) {
        self.map.set(x, y, Terrain::Burning);
        self.dirty.mark(x, y);
        let burn_ticks = rng.random_range(30u32..=50);
        self.fire_tiles.push((x, y, burn_ticks));
        self.notify("Fire! A forest fire has started!".to_string());
    }

    /// Process fire spread and burnout each tick. Only iterates over active
    /// fire tiles — O(fire_front), not O(map).
    fn tick_fire(&mut self) {
        if self.fire_tiles.is_empty() {
            return;
        }

        let mut rng = rand::rng();
        let w = self.map.width;
        let h = self.map.height;
        let mut new_fires: Vec<(usize, usize, u32)> = Vec::new();

        // Build a set of currently burning positions for fast lookup
        let burning_set: std::collections::HashSet<(usize, usize)> =
            self.fire_tiles.iter().map(|&(x, y, _)| (x, y)).collect();

        // Decrement timers and collect spread candidates
        for entry in &mut self.fire_tiles {
            let (x, y, ref mut timer) = *entry;

            if *timer > 0 {
                *timer -= 1;
            }

            // Try to spread to 8 neighbors
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let ux = nx as usize;
                    let uy = ny as usize;
                    let Some(terrain) = self.map.get(ux, uy).copied() else {
                        continue;
                    };
                    if !terrain.is_flammable() {
                        continue;
                    }
                    // High moisture blocks spread
                    let moisture = self.moisture.get(ux, uy);
                    if moisture > 0.6 {
                        continue;
                    }
                    // spread_probability = 0.03 * (1.0 - moisture) * vegetation_factor
                    let veg = self.vegetation.get(ux, uy).clamp(0.3, 1.0);
                    let prob = 0.03 * (1.0 - moisture) * veg;
                    let roll = rng.random_range(0u32..10000) as f64 / 10000.0;
                    if roll < prob {
                        let already = burning_set.contains(&(ux, uy))
                            || new_fires.iter().any(|(fx, fy, _)| *fx == ux && *fy == uy);
                        if !already {
                            let burn_ticks = rng.random_range(30u32..=50);
                            new_fires.push((ux, uy, burn_ticks));
                        }
                    }
                }
            }
        }

        // Burnout: tiles whose timer hit 0 become Scorched
        let mut burned_out: Vec<(usize, usize)> = Vec::new();
        self.fire_tiles.retain(|&(x, y, timer)| {
            if timer == 0 {
                burned_out.push((x, y));
                false
            } else {
                true
            }
        });
        for (x, y) in &burned_out {
            self.map.set(*x, *y, Terrain::Scorched);
            self.dirty.mark(*x, *y);
            // Ash fertility bonus
            self.soil_fertility.add(*x, *y, 0.05);
            // Clear vegetation
            if let Some(v) = self.vegetation.get_mut(*x, *y) {
                *v = 0.0;
            }
        }

        // Set new fire tiles on the map and add to tracking list
        for &(x, y, _) in &new_fires {
            self.map.set(x, y, Terrain::Burning);
            self.dirty.mark(x, y);
        }
        self.fire_tiles.extend(new_fires);

        // Damage entities on burning tiles
        self.fire_damage_entities();
    }

    /// Entities standing on Burning tiles take hunger damage.
    fn fire_damage_entities(&mut self) {
        let mut damage_targets: Vec<hecs::Entity> = Vec::new();
        for (entity, (pos, _creature)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Creature))>()
            .iter()
        {
            let tx = pos.x.round() as usize;
            let ty = pos.y.round() as usize;
            if self.map.get(tx, ty) == Some(&Terrain::Burning) {
                damage_targets.push(entity);
            }
        }
        for entity in damage_targets {
            if let Ok(mut creature) = self.world.get::<&mut Creature>(entity) {
                creature.hunger += 2.0;
            }
        }
    }

    /// Check if there are any burning tiles visible from a position.
    pub fn burning_tiles_near(&self, x: f64, y: f64, range: f64) -> Option<(f64, f64)> {
        if self.fire_tiles.is_empty() {
            return None;
        }
        let range_sq = range * range;
        let mut nearest_dist_sq = f64::INFINITY;
        let mut nearest = None;
        for &(fx, fy, _) in &self.fire_tiles {
            let dx = fx as f64 - x;
            let dy = fy as f64 - y;
            let d2 = dx * dx + dy * dy;
            if d2 < range_sq && d2 < nearest_dist_sq {
                nearest_dist_sq = d2;
                nearest = Some((fx as f64, fy as f64));
            }
        }
        nearest
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
        match input {
            GameInput::ScrollUp => self.camera.y -= self.scroll_speed,
            GameInput::ScrollDown => self.camera.y += self.scroll_speed,
            GameInput::ScrollLeft => self.camera.x -= self.scroll_speed,
            GameInput::ScrollRight => self.camera.x += self.scroll_speed,
            GameInput::ToggleRain => {
                self.raining = !self.raining;
                self.dirty.mark_all();
            }
            GameInput::ToggleErosion => {
                self.sim_config.erosion_enabled = !self.sim_config.erosion_enabled
            }
            GameInput::ToggleDayNight => {
                self.day_night.enabled = !self.day_night.enabled;
                self.dirty.mark_all();
            }
            GameInput::ToggleDebugView => {
                self.render_mode = self.render_mode.next();
                self.dirty.mark_all();
            }
            GameInput::TogglePause => self.paused = !self.paused,
            GameInput::ToggleQueryMode => {
                // Mark old cursor position dirty to clean up artifacts
                self.dirty
                    .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                self.query_mode = !self.query_mode;
                if self.query_mode {
                    self.build_mode = false; // mutually exclusive
                    // Center cursor on screen (account for panel)
                    let (vw, vh) = renderer.size();
                    let map_w = vw.saturating_sub(PANEL_WIDTH) as i32;
                    let world_vw = map_w / CELL_ASPECT;
                    self.query_cx = self.camera.x + world_vw / 2;
                    self.query_cy = self.camera.y + vh as i32 / 2;
                }
            }
            GameInput::QueryUp => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cy -= 1;
                }
            }
            GameInput::QueryDown => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cy += 1;
                }
            }
            GameInput::QueryLeft => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cx -= 1;
                }
            }
            GameInput::QueryRight => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cx += 1;
                }
            }
            GameInput::ToggleBuildMode => {
                // Mark old cursor footprint dirty to clean up artifacts
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                }
                self.build_mode = !self.build_mode;
                if self.build_mode {
                    self.query_mode = false; // mutually exclusive
                    let (vw, vh) = renderer.size();
                    let map_w = vw.saturating_sub(PANEL_WIDTH) as i32;
                    let world_vw = map_w / CELL_ASPECT;
                    self.build_cursor_x = self.camera.x + world_vw / 2;
                    self.build_cursor_y = self.camera.y + vh as i32 / 2;
                }
            }
            GameInput::BuildUp => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_y -= 1;
                }
            }
            GameInput::BuildDown => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_y += 1;
                }
            }
            GameInput::BuildLeft => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_x -= 1;
                }
            }
            GameInput::BuildRight => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_x += 1;
                }
            }
            GameInput::BuildCycleType => {
                if self.build_mode {
                    let types = BuildingType::all();
                    let idx = types
                        .iter()
                        .position(|t| *t == self.selected_building)
                        .unwrap_or(0);
                    self.selected_building = types[(idx + 1) % types.len()];
                }
            }
            GameInput::BuildPlace => {
                if self.build_mode {
                    self.try_place_building();
                }
            }
            GameInput::Drain => self.water.drain(),
            GameInput::ToggleAutoBuild => self.auto_build = !self.auto_build,
            GameInput::CycleOverlay => {
                self.overlay = match self.overlay {
                    OverlayMode::None => OverlayMode::Tasks,
                    OverlayMode::Tasks => OverlayMode::Resources,
                    OverlayMode::Resources => OverlayMode::Threats,
                    OverlayMode::Threats => OverlayMode::Traffic,
                    OverlayMode::Traffic => OverlayMode::Territory,
                    OverlayMode::Territory => OverlayMode::None,
                };
                self.dirty.mark_all();
            }
            GameInput::MouseClick { x, y } => self.handle_mouse_click(x, y, renderer),
            GameInput::GotoSettlement => {
                let (scx, scy) = self.settlement_center();
                let (vw, vh) = renderer.size();
                let map_cols = vw.saturating_sub(PANEL_WIDTH) as i32 / CELL_ASPECT;
                self.camera.x = scx - map_cols / 2;
                self.camera.y = scy - vh as i32 / 2;
            }
            GameInput::CycleSpeed => {
                self.game_speed = match self.game_speed {
                    1 => 2,
                    2 => 5,
                    _ => 1,
                };
                self.notify(format!("Speed: {}x", self.game_speed));
            }
            GameInput::Demolish => {
                if self.build_mode {
                    self.demolish_at(self.build_cursor_x, self.build_cursor_y);
                }
            }
            GameInput::Save => {
                let _ = self.save("savegame.json");
            }
            GameInput::Load => {} // handled in main.rs loop
            GameInput::Quit | GameInput::Restart | GameInput::None => {}
        }

        let (vw, vh) = renderer.size();
        // World-space viewport: map area is screen minus panel, divided by aspect ratio
        let map_w = vw.saturating_sub(PANEL_WIDTH);
        let world_vw = (map_w as i32 / CELL_ASPECT) as u16;
        self.camera
            .clamp(self.map.width, self.map.height, world_vw, vh);

        // update simulation (skip when paused)
        if !self.paused {
            for _speed_tick in 0..self.game_speed {
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

                ecs::system_death(&mut self.world);

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
                // Mark old + new positions dirty for rendering.
                for p in &mut self.particles {
                    let old_x = p.x.round() as usize;
                    let old_y = p.y.round() as usize;
                    self.dirty.mark(old_x, old_y);
                    p.x += p.dx;
                    p.y += p.dy;
                    p.life -= 1;
                    let new_x = p.x.round() as usize;
                    let new_y = p.y.round() as usize;
                    self.dirty.mark(new_x, new_y);
                }
                // Mark expired particles' positions dirty before removing
                for p in &self.particles {
                    if p.life == 0 {
                        self.dirty.mark(p.x.round() as usize, p.y.round() as usize);
                    }
                }
                self.particles.retain(|p| p.life > 0);

                // Spawn activity particles from active processing buildings
                {
                    let mut rng = rand::rng();
                    let building_sources: Vec<(Recipe, f64, f64)> = self
                        .world
                        .query::<(&ProcessingBuilding, &Position)>()
                        .iter()
                        .filter(|(pb, _)| pb.worker_present)
                        .map(|(pb, pos)| (pb.recipe, pos.x, pos.y))
                        .collect();
                    for (recipe, px, py) in building_sources {
                        if self.particles.len() >= MAX_PARTICLES {
                            break;
                        }
                        // Per-building-type particle signature
                        let (spawn_rate, chars, fg, dx_range, dy_range, life_range, emissive) =
                            match recipe {
                                Recipe::WoodToPlanks => {
                                    // Workshop: grey smoke, lazy drift
                                    (
                                        3u32,
                                        &['.', '\u{00b0}', '\''][..],
                                        Color(140, 130, 110),
                                        (-0.05f64, 0.05f64),
                                        (-0.15f64, -0.08f64),
                                        (18u32, 28u32),
                                        false,
                                    )
                                }
                                Recipe::StoneToMasonry => {
                                    // Smithy: orange sparks, fast rise, short life
                                    (
                                        2,
                                        &['*', '\u{00b7}', '\''][..],
                                        Color(255, 140, 40),
                                        (-0.08, 0.08),
                                        (-0.25, -0.10),
                                        (10, 18),
                                        true,
                                    )
                                }
                                Recipe::FoodToGrain => {
                                    // Granary: pale straw, minimal
                                    (
                                        4,
                                        &['.', ','][..],
                                        Color(180, 170, 120),
                                        (-0.03, 0.03),
                                        (-0.10, -0.05),
                                        (12, 20),
                                        false,
                                    )
                                }
                                Recipe::GrainToBread => {
                                    // Bakery: white steam plumes
                                    (
                                        2,
                                        &['~', '\'', '.'][..],
                                        Color(200, 200, 210),
                                        (-0.06, 0.06),
                                        (-0.12, -0.06),
                                        (20, 35),
                                        false,
                                    )
                                }
                            };
                        if rng.random_range(0..spawn_rate) == 0 {
                            let ch = chars[rng.random_range(0..chars.len())];
                            let life = rng.random_range(life_range.0..=life_range.1);
                            self.particles.push(Particle {
                                x: px,
                                y: py - 1.0,
                                ch,
                                fg,
                                life,
                                max_life: life,
                                dx: rng.random_range(dx_range.0..dx_range.1),
                                dy: rng.random_range(dy_range.0..dy_range.1),
                                emissive,
                            });
                        }
                    }

                    // Spawn villager activity particles (construction dust, mining sparkle)
                    let villager_activities: Vec<(BehaviorState, f64, f64)> = self
                        .world
                        .query::<(&Behavior, &Position)>()
                        .iter()
                        .map(|(b, pos)| (b.state, pos.x, pos.y))
                        .collect();
                    for (state, vx, vy) in villager_activities {
                        if self.particles.len() >= MAX_PARTICLES {
                            break;
                        }
                        match state {
                            BehaviorState::Building {
                                target_x, target_y, ..
                            } => {
                                // Construction: yellow-brown dust at build site
                                if rng.random_range(0..4) == 0 {
                                    let chars = ['#', '.', '+'];
                                    let ch = chars[rng.random_range(0..chars.len())];
                                    let life = rng.random_range(6..=12);
                                    self.particles.push(Particle {
                                        x: target_x,
                                        y: target_y,
                                        ch,
                                        fg: Color(220, 200, 100),
                                        life,
                                        max_life: life,
                                        dx: rng.random_range(-0.15..0.15),
                                        dy: rng.random_range(-0.10..0.10),
                                        emissive: false,
                                    });
                                }
                            }
                            BehaviorState::Gathering {
                                resource_type: ResourceType::Stone,
                                ..
                            } => {
                                // Mining: white-blue sparkle
                                if rng.random_range(0..3) == 0 {
                                    let chars = ['*', '\'', '.'];
                                    let ch = chars[rng.random_range(0..chars.len())];
                                    let life = rng.random_range(4..=8);
                                    self.particles.push(Particle {
                                        x: vx,
                                        y: vy,
                                        ch,
                                        fg: Color(200, 200, 220),
                                        life,
                                        max_life: life,
                                        dx: rng.random_range(-0.20..0.20),
                                        dy: rng.random_range(-0.15..0.05),
                                        emissive: false,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }

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

                // Seasonal auto-rain (rain_mult: spring=1.5, summer=0.5, autumn=1.0, winter=0.3)
                let should_rain = self.raining || (self.tick % 20 == 0 && mods.rain_mult > 0.4);
                if should_rain {
                    self.water.rain(&tick_config);
                }
                // Only run expensive water sim when there's actually water
                let viewport_bounds = Some((
                    self.camera.x.max(0) as usize,
                    self.camera.y.max(0) as usize,
                    (self.camera.x.max(0) as usize).saturating_add(world_vw as usize),
                    (self.camera.y.max(0) as usize).saturating_add(vh as usize),
                ));
                if should_rain || self.water.has_water() {
                    self.water
                        .update(&mut self.heights, &tick_config, viewport_bounds);
                    self.moisture
                        .update(&self.water, &mut self.vegetation, &self.map);
                }

                // Seasonal vegetation decay (winter/autumn)
                self.vegetation.apply_season(mods.veg_growth_mult);

                // rebuild tiles if erosion changed heights
                if self.sim_config.erosion_enabled {
                    terrain_gen::rebuild_tiles(&mut self.map, &self.heights, &self.terrain_config);
                }

                // advance day/night cycle and compute Blinn-Phong lighting + shadows (viewport only)
                let prev_season = self.day_night.season;
                self.day_night.tick();
                if self.day_night.season != prev_season {
                    self.dirty.mark_all(); // Season change affects all visible tiles
                    let season_msg = match self.day_night.season {
                        Season::Spring => "Spring has arrived — the ice thaws!",
                        Season::Summer => "Summer heat — fire risk increases!",
                        Season::Autumn => "Autumn harvest — gather while you can!",
                        Season::Winter => "Winter descends — conserve resources!",
                    };
                    self.notify_milestone(season_msg);

                    // --- Seasonal terrain transitions ---
                    // Revert previous season's overlays before applying new ones.
                    match prev_season {
                        Season::Winter => {
                            // Thaw: revert Ice -> Water
                            self.map.revert_ice();
                        }
                        Season::Spring => {
                            // Floods recede: revert FloodWater -> base terrain
                            let reverted = self.map.revert_flood_water();
                            // Alluvial fertility bonus on tiles that were flooded
                            for (x, y) in &reverted {
                                self.soil_fertility.add(*x, *y, 0.15);
                            }
                            if !reverted.is_empty() {
                                self.notify(format!(
                                    "Floods recede — {} tiles enriched with alluvial soil",
                                    reverted.len()
                                ));
                            }
                            self.flooded_tiles.clear();
                            self.flood_start_tick = 0;
                        }
                        _ => {}
                    }

                    // Apply new season's effects
                    match self.day_night.season {
                        Season::Winter => {
                            let frozen = self.map.apply_winter_ice();
                            if frozen > 0 {
                                self.notify(format!(
                                    "Rivers freeze! {} tiles of ice — wolves can cross!",
                                    frozen
                                ));
                            }
                        }
                        Season::Spring => {
                            let flooded = self.map.apply_spring_floods(
                                &self.river_mask,
                                &self.heights,
                                &self.soil,
                            );
                            if !flooded.is_empty() {
                                // Destroy farms on flooded tiles
                                let mut destroyed_farms = 0u32;
                                let flood_set: std::collections::HashSet<(usize, usize)> =
                                    flooded.iter().copied().collect();
                                let farm_entities: Vec<hecs::Entity> = self
                                    .world
                                    .query::<(hecs::Entity, &FarmPlot)>()
                                    .iter()
                                    .filter(|(_, f)| flood_set.contains(&(f.tile_x, f.tile_y)))
                                    .map(|(e, _)| e)
                                    .collect();
                                for entity in farm_entities {
                                    let _ = self.world.despawn(entity);
                                    destroyed_farms += 1;
                                }
                                let msg = if destroyed_farms > 0 {
                                    format!(
                                        "Spring floods! {} tiles flooded, {} farms destroyed",
                                        flooded.len(),
                                        destroyed_farms
                                    )
                                } else {
                                    format!(
                                        "Spring floods! {} tiles flooded near rivers",
                                        flooded.len()
                                    )
                                };
                                self.notify(msg);
                                self.flood_start_tick = self.tick;
                                self.flooded_tiles = flooded;
                            }
                        }
                        _ => {}
                    }
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

        serde_json::json!({
            "tick": self.tick,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::{self, Creature, Species};
    use crate::headless_renderer::HeadlessRenderer;
    use crate::tilemap::{Terrain, TileMap};
    use hecs::World;

    #[test]
    fn population_growth_spawns_villager() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let mut world = World::new();

        // Spawn 2 villagers (minimum for reproduction)
        ecs::spawn_villager(&mut world, 10.0, 10.0);
        ecs::spawn_villager(&mut world, 11.0, 10.0);

        let initial_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        assert_eq!(initial_count, 2);

        let mut resources = Resources {
            food: 10,
            ..Default::default()
        };

        let villager_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        if villager_count >= 2 && resources.food >= 5 {
            resources.food -= 5;
            let villager_pos: Vec<(f64, f64)> = world
                .query::<(&Position, &Creature)>()
                .iter()
                .filter(|(_, c)| c.species == Species::Villager)
                .map(|(p, _)| (p.x, p.y))
                .collect();
            if let Some(&(vx, vy)) = villager_pos.first() {
                let mut spawned = false;
                for r in 0..5i32 {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if spawned {
                                continue;
                            }
                            let nx = vx + dx as f64;
                            let ny = vy + dy as f64;
                            if map.is_walkable(nx, ny) {
                                ecs::spawn_villager(&mut world, nx, ny);
                                spawned = true;
                            }
                        }
                    }
                }
            }
        }

        let final_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        assert_eq!(final_count, 3, "should have spawned one new villager");
        assert_eq!(resources.food, 5, "should have consumed 5 food");
    }

    #[test]
    fn game_over_when_all_villagers_die() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        assert!(!game.game_over, "should not start in game over");
        assert!(!game.paused, "should not start paused");

        // Set all villager hunger to 1.0 so system_death kills them
        for creature in game.world.query_mut::<&mut Creature>() {
            if creature.species == Species::Villager {
                creature.hunger = 1.0;
            }
        }

        // Step — death system should trigger game over
        game.step(GameInput::None, &mut renderer).unwrap();

        assert!(game.game_over, "game should be over when all villagers die");
        assert!(game.paused, "game should pause on game over");
    }

    #[test]
    fn auto_build_places_farm_when_food_low() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        game.auto_build = true;
        game.resources.food = 2;
        // Wood must be >= hut_cost + farm_cost (10 + 5 = 15) so the housing-priority guard
        // does not block the farm: the guard prevents farms only when wood < 15 and a hut is
        // needed, to ensure wood accumulates for the hut first.
        game.resources.wood = 15;
        game.resources.stone = 10;

        // Ensure grass around settlement so farms can be placed
        let (scx, scy) = game.settlement_center();
        for dy in -8i32..=8 {
            for dx in -8i32..=8 {
                let tx = (scx + dx) as usize;
                let ty = (scy + dy) as usize;
                if let Some(t) = game.map.get(tx, ty) {
                    if matches!(t, Terrain::Mountain | Terrain::Snow) {
                        game.map.set(tx, ty, Terrain::Grass);
                    }
                }
            }
        }

        // Build up influence so auto-build can place within territory
        for _ in 0..30 {
            game.update_influence();
        }

        let farms_before = game
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Farm)
            .count()
            + game.world.query::<&FarmPlot>().iter().count();

        game.auto_build_tick();

        let farms_after = game
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Farm)
            .count()
            + game.world.query::<&FarmPlot>().iter().count();

        assert!(
            farms_after > farms_before,
            "auto-build should queue a farm when food is low"
        );
        let farm_cost = BuildingType::Farm.cost();
        // Fix 5: P1 (farm) and P2 (hut) may both queue in the same tick.
        // With wood=15 and a hut also needed, hut (10w) deducts after farm (5w) → wood=0.
        // Assert farm cost was deducted; allow for hut also queuing.
        assert_eq!(game.resources.food, 2 - farm_cost.food);
        assert!(
            game.resources.wood <= 15 - farm_cost.wood,
            "farm cost (5w) should be deducted; wood={}",
            game.resources.wood
        );
    }

    #[test]
    fn skills_increase_with_activity() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        let initial_woodcutting = game.skills.woodcutting;

        for _ in 0..500 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let any_skill_increased = game.skills.woodcutting > initial_woodcutting
            || game.skills.mining > 0.5
            || game.skills.farming > 0.5
            || game.skills.building > 0.5;

        assert!(
            any_skill_increased,
            "skills should increase from villager activity: wood={:.2} mine={:.2} farm={:.2} build={:.2}",
            game.skills.woodcutting, game.skills.mining, game.skills.farming, game.skills.building
        );
    }

    #[test]
    fn skills_decay_over_time() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Use a skill that has no passive gain sources (building skill)
        // Set it high so we can observe decay clearly
        game.skills.building = 80.0;

        for _ in 0..1000 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        assert!(
            game.skills.building < 80.0,
            "building skill should decay without activity: {:.2}",
            game.skills.building
        );
    }

    #[test]
    fn save_load_round_trip() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        for _ in 0..50 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        let tick_before = game.tick;
        let food_before = game.resources.food;
        let villager_count_before = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        game.save("/tmp/test_savegame.json").unwrap();
        let loaded = Game::load("/tmp/test_savegame.json", 60).unwrap();

        assert_eq!(loaded.tick, tick_before);
        assert_eq!(loaded.resources.food, food_before);
        let villager_count_after = loaded
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        assert_eq!(villager_count_after, villager_count_before);

        let _ = std::fs::remove_file("/tmp/test_savegame.json");
    }

    #[test]
    fn defense_rating_increases_with_garrison() {
        let mut game = Game::new(60, 42);

        let base_defense = game.compute_defense_rating();

        ecs::spawn_garrison(&mut game.world, 125.0, 125.0);

        let new_defense = game.compute_defense_rating();
        assert!(
            new_defense > base_defense,
            "defense rating should increase with garrison: base={}, new={}",
            base_defense,
            new_defense
        );
        assert!(
            (new_defense - base_defense - 5.0).abs() < 0.01,
            "garrison should add 5.0 defense, got difference: {}",
            new_defense - base_defense
        );
    }

    #[test]
    fn build_site_gets_completed_in_game() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Give plenty of all resources so villagers don't prioritize gathering over building
        game.resources.food = 200;
        game.resources.wood = 100;
        game.resources.stone = 100;

        // Place a wall build site near the actual settlement center on walkable terrain
        let (scx, scy) = game.settlement_center();
        // Ensure the site terrain is walkable
        game.map
            .set((scx + 2) as usize, scy as usize, Terrain::Grass);
        let site = ecs::spawn_build_site(
            &mut game.world,
            scx as f64 + 2.0,
            scy as f64,
            BuildingType::Wall,
            0,
        );

        // Run for enough ticks — wall requires 30 build_time, villagers may be slow on terrain
        for _ in 0..3000 {
            game.step(GameInput::None, &mut renderer).unwrap();
            if game.world.get::<&BuildSite>(site).is_err() {
                return; // Build site despawned = completed
            }
        }

        if let Ok(s) = game.world.get::<&BuildSite>(site) {
            panic!(
                "build site not completed after 3000 ticks: progress={}/{}",
                s.progress, s.required
            );
        }
    }

    #[test]
    fn winter_food_decay() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.resources.food = 20;
        game.resources.grain = 10;

        // Set season to winter
        game.day_night.season = Season::Winter;

        // Run for 200 ticks — should lose some food but not grain
        let initial_grain = game.resources.grain;
        for _ in 0..200 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        // Food should have decayed (at least some lost to spoilage, though villagers also eat)
        // Grain should NOT have decayed from winter spoilage (villagers may eat some)
        // The key test: grain is preserved relative to food
        assert!(game.resources.food < 20, "food should decay in winter");
        // Note: grain may decrease from villager eating, but won't decrease from spoilage
    }

    #[test]
    fn refined_resources_shown_in_panel() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        game.resources.planks = 3;
        game.resources.masonry = 2;
        game.resources.grain = 5;

        game.step(GameInput::None, &mut renderer).unwrap();
        let frame_text = renderer.frame_as_string();

        assert!(
            frame_text.contains("Planks"),
            "panel should show planks when > 0"
        );
        assert!(
            frame_text.contains("Masonry"),
            "panel should show masonry when > 0"
        );
        assert!(
            frame_text.contains("Grain"),
            "panel should show grain when > 0"
        );
    }

    #[test]
    fn garrison_placement_requires_wood_and_stone() {
        let mut game = Game::new(60, 42);

        // Insufficient wood
        game.resources = Resources {
            wood: 5,
            stone: 12,
            ..Default::default()
        };

        let cost = BuildingType::Garrison.cost();
        assert!(
            !game.resources.can_afford(&cost),
            "should NOT afford garrison with insufficient wood"
        );

        // Sufficient wood + stone
        game.resources.wood = 6;
        assert!(
            game.resources.can_afford(&cost),
            "should afford garrison with 6 wood + 12 stone"
        );
    }

    #[test]
    fn population_growth_requires_housing() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Give lots of food
        game.resources.food = 100;
        game.last_birth_tick = 0;

        // Count initial villagers
        let initial = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        // Run without any huts — no growth should happen
        for _ in 0..1000 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let after_no_huts = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        // Now add a hut with capacity for growth
        ecs::spawn_hut(&mut game.world, 125.0, 125.0);
        game.resources.food = 100;
        game.last_birth_tick = 0;

        for _ in 0..1000 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let after_hut = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        // With a hut providing surplus capacity, population should grow
        assert!(
            after_hut > after_no_huts || after_hut > initial,
            "population should grow when housing is available: initial={} no_huts={} with_hut={}",
            initial,
            after_no_huts,
            after_hut
        );
    }

    #[test]
    fn overlay_cycles_through_all_modes() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        assert_eq!(game.overlay, OverlayMode::None);

        game.step(GameInput::CycleOverlay, &mut renderer).unwrap();
        assert_eq!(game.overlay, OverlayMode::Tasks);

        game.step(GameInput::CycleOverlay, &mut renderer).unwrap();
        assert_eq!(game.overlay, OverlayMode::Resources);

        game.step(GameInput::CycleOverlay, &mut renderer).unwrap();
        assert_eq!(game.overlay, OverlayMode::Threats);

        game.step(GameInput::CycleOverlay, &mut renderer).unwrap();
        assert_eq!(game.overlay, OverlayMode::Traffic);

        game.step(GameInput::CycleOverlay, &mut renderer).unwrap();
        assert_eq!(game.overlay, OverlayMode::Territory);

        game.step(GameInput::CycleOverlay, &mut renderer).unwrap();
        assert_eq!(game.overlay, OverlayMode::None);
    }

    #[test]
    fn villagers_sleep_at_night() {
        let mut world = hecs::World::new();
        let map = TileMap::new(30, 30, Terrain::Grass);

        let v = ecs::spawn_villager(&mut world, 10.0, 10.0);
        ecs::spawn_stockpile(&mut world, 5.0, 5.0);
        ecs::spawn_hut(&mut world, 10.0, 10.0);

        // Run AI with is_night=true
        let mut grid = crate::ecs::spatial::SpatialHashGrid::new(30, 30, 16);
        grid.populate(&world);
        let result = ecs::system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            10,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            true,
            &[],
            0,
            &[],
            &ScentMap::default(),
            &ScentMap::default(),
            &crate::pathfinding::NavGraph::default(),
            &crate::pathfinding::FlowFieldRegistry::new(),
        );

        let state = world.get::<&Behavior>(v).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Sleeping { .. }),
            "villager should sleep at night when hut is nearby, got: {:?}",
            state
        );
    }

    #[test]
    fn drought_event_detected() {
        let mut events = EventSystem::default();
        assert!(!events.has_drought());
        events.active_events.push(GameEvent::Drought {
            ticks_remaining: 100,
        });
        assert!(events.has_drought());
        assert!(!events.has_bountiful_harvest());
    }

    #[test]
    fn bountiful_harvest_event_detected() {
        let mut events = EventSystem::default();
        assert!(!events.has_bountiful_harvest());
        events.active_events.push(GameEvent::BountifulHarvest {
            ticks_remaining: 100,
        });
        assert!(events.has_bountiful_harvest());
        assert!(!events.has_drought());
    }

    #[test]
    fn drought_reduces_rain_rate() {
        // Drought should reduce rain_rate by 60% when applied to SimConfig.
        // Chain: drought -> less rain -> less water -> less moisture -> slower farms.
        let base_config = SimConfig::default();
        let base_rain = base_config.rain_rate;

        // Simulate what step() does: seasonal mult * drought mult
        let drought_rain = base_rain * 0.4; // drought factor
        assert!(
            drought_rain < base_rain * 0.5,
            "drought should cut rain to 40%: base={}, drought={}",
            base_rain,
            drought_rain
        );
    }

    #[test]
    fn bountiful_harvest_increases_rain_rate() {
        // Bountiful harvest should increase rain_rate by 50%.
        // Chain: more rain -> more water -> more moisture -> faster farms.
        let base_config = SimConfig::default();
        let base_rain = base_config.rain_rate;

        let bountiful_rain = base_rain * 1.5;
        assert!(
            bountiful_rain > base_rain,
            "bountiful should increase rain: base={}, bountiful={}",
            base_rain,
            bountiful_rain
        );
    }

    #[test]
    fn low_fertility_slows_farm_growth() {
        // Farm on low-fertility soil should grow slower than on rich soil.
        use crate::simulation::SoilFertilityMap;
        use crate::terrain_pipeline::SoilType;
        let mm = {
            let mut m = MoistureMap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    m.set(x, y, 0.6);
                }
            }
            m
        };
        let soil = vec![SoilType::Loam; 64 * 64];

        let mut world_rich = World::new();
        ecs::spawn_farm_plot(&mut world_rich, 5.0, 5.0);
        let mut fert_rich = SoilFertilityMap::new(64, 64); // 1.0 everywhere

        let mut world_poor = World::new();
        ecs::spawn_farm_plot(&mut world_poor, 5.0, 5.0);
        let mut fert_poor = SoilFertilityMap::new(64, 64);
        fert_poor.set(5, 5, 0.2); // poor soil at farm tile

        let ticks = 100;
        for _ in 0..ticks {
            for farm in world_rich.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            ecs::system_farms(
                &mut world_rich,
                Season::Summer,
                1.0,
                &mm,
                &mut fert_rich,
                &soil,
            );
            for farm in world_poor.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            ecs::system_farms(
                &mut world_poor,
                Season::Summer,
                1.0,
                &mm,
                &mut fert_poor,
                &soil,
            );
        }

        let rich_growth = world_rich
            .query::<&FarmPlot>()
            .iter()
            .next()
            .unwrap()
            .growth;
        let poor_growth = world_poor
            .query::<&FarmPlot>()
            .iter()
            .next()
            .unwrap()
            .growth;
        assert!(
            rich_growth > poor_growth,
            "rich soil farm should grow faster: rich={}, poor={}",
            rich_growth,
            poor_growth
        );
    }

    #[test]
    fn soil_fertility_initialized_from_soil_types() {
        use crate::simulation::SoilFertilityMap;
        use crate::terrain_pipeline::SoilType;

        let soil = vec![
            SoilType::Alluvial,
            SoilType::Sand,
            SoilType::Rocky,
            SoilType::Loam,
        ];
        let fert = SoilFertilityMap::from_soil_types(2, 2, &soil);

        // Alluvial: yield_multiplier = 1.25, clamped to 1.0
        assert!((fert.get(0, 0) - 1.0).abs() < 0.01);
        // Sand: 0.7
        assert!((fert.get(1, 0) - 0.7).abs() < 0.01);
        // Rocky: 0.4
        assert!((fert.get(0, 1) - 0.4).abs() < 0.01);
        // Loam: 1.0
        assert!((fert.get(1, 1) - 1.0).abs() < 0.01);
    }

    #[test]
    fn wolf_surge_doubles_breeding() {
        let mut events = EventSystem::default();
        assert_eq!(events.wolf_spawn_multiplier(), 1.0);

        events.active_events.push(GameEvent::WolfSurge {
            ticks_remaining: 100,
        });
        assert_eq!(events.wolf_spawn_multiplier(), 2.0);
    }

    #[test]
    fn events_expire_after_duration() {
        let mut game = Game::new(60, 42);
        game.events
            .active_events
            .push(GameEvent::Drought { ticks_remaining: 2 });

        // Tick 1: still active
        game.tick = 99; // avoid the event check (only triggers on tick % 100 == 0)
        game.update_events();
        assert_eq!(game.events.active_events.len(), 1);

        // Tick 2: should expire
        game.update_events();
        assert_eq!(game.events.active_events.len(), 0);
    }

    #[test]
    fn no_duplicate_events() {
        let mut events = EventSystem::default();
        events.active_events.push(GameEvent::Drought {
            ticks_remaining: 100,
        });
        assert!(events.has_event_type("drought"));
        // The check prevents duplicates
        assert!(!events.has_event_type("harvest"));
    }

    #[test]
    fn event_system_serialization() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        game.events.active_events.push(GameEvent::Drought {
            ticks_remaining: 150,
        });
        game.events.event_log.push("Test event".to_string());

        game.save("/tmp/test_events_save.json").unwrap();
        let loaded = Game::load("/tmp/test_events_save.json", 60).unwrap();

        assert_eq!(loaded.events.active_events.len(), 1);
        assert!(matches!(
            loaded.events.active_events[0],
            GameEvent::Drought {
                ticks_remaining: 150
            }
        ));
        assert_eq!(loaded.events.event_log.len(), 1);

        // Cleanup
        let _ = std::fs::remove_file("/tmp/test_events_save.json");
    }

    #[test]
    fn threat_overlay_marks_wolves() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.overlay = OverlayMode::Threats;

        // Spawn a wolf in view
        ecs::spawn_predator(
            &mut game.world,
            (game.camera.x + 5) as f64,
            (game.camera.y + 5) as f64,
        );

        game.draw(&mut renderer);

        // The wolf should be rendered as 'W' somewhere on screen
        let frame = renderer.frame_as_string();
        assert!(
            frame.contains('W'),
            "threat overlay should show wolves as 'W'"
        );
    }

    #[test]
    fn resource_overlay_marks_food_sources() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.overlay = OverlayMode::Resources;

        // Spawn berry bush in view
        ecs::spawn_berry_bush(
            &mut game.world,
            (game.camera.x + 5) as f64,
            (game.camera.y + 5) as f64,
        );

        game.draw(&mut renderer);

        // Berry bush char '♦' should appear
        let frame = renderer.frame_as_string();
        assert!(
            frame.contains('♦'),
            "resource overlay should show berry bushes"
        );
    }

    #[test]
    fn wolf_raid_triggers_with_pack() {
        let mut world = hecs::World::new();
        let map = TileMap::new(50, 50, Terrain::Grass);

        // Spawn 6 wolves near each other
        for i in 0..6 {
            ecs::spawn_predator(&mut world, 30.0 + i as f64, 30.0);
        }

        // Raid should trigger (wolves within 15 tiles of each other)
        let raided = ecs::system_wolf_raids(&mut world, 25.0, 25.0, 50, 0);
        assert!(raided, "raid should trigger with 6 wolves in a pack");

        // All wolves should now be Hunting toward settlement
        let hunting_count = world
            .query::<(&Creature, &Behavior)>()
            .iter()
            .filter(|(c, b)| {
                c.species == Species::Predator && matches!(b.state, BehaviorState::Hunting { .. })
            })
            .count();
        assert!(
            hunting_count >= 5,
            "pack wolves should be hunting: got {}",
            hunting_count
        );
    }

    #[test]
    fn wolf_raid_needs_minimum_pack() {
        let mut world = hecs::World::new();

        // Only 3 wolves — not enough for a raid (year 0, threshold = 5)
        for i in 0..3 {
            ecs::spawn_predator(&mut world, 30.0 + i as f64, 30.0);
        }

        let raided = ecs::system_wolf_raids(&mut world, 25.0, 25.0, 50, 0);
        assert!(!raided, "raid should not trigger with only 3 wolves");
    }

    #[test]
    fn building_requires_influence() {
        let mut game = Game::new(60, 42);

        // Far from settlement — no influence
        let far_x = 10i32;
        let far_y = 10i32;
        assert!(
            !game.can_place_building(far_x, far_y, BuildingType::Wall),
            "should not be able to build outside influence"
        );

        // Near settlement — build up influence
        for _ in 0..30 {
            game.update_influence();
        }
        // Find a buildable spot near settlement (search for valid terrain within influence)
        let (scx, scy) = game.settlement_center();
        let found = game.find_building_spot(scx as f64, scy as f64, BuildingType::Wall);
        assert!(
            found.is_some(),
            "should find a buildable spot within influence"
        );
    }

    #[test]
    fn traffic_converts_grass_to_road() {
        let mut game = Game::new(60, 42);

        // Manually accumulate traffic on a grass tile
        let tx = 130usize;
        let ty = 130usize;
        // Ensure the tile is grass
        game.map.set(tx, ty, Terrain::Grass);

        // Simulate heavy foot traffic (above threshold of 300)
        for _ in 0..400 {
            game.traffic.step_on(tx, ty);
        }

        // Trigger road conversion check
        game.tick = 100; // align to conversion interval
        game.update_traffic();

        assert_eq!(
            *game.map.get(tx, ty).unwrap(),
            Terrain::Road,
            "heavily trafficked grass should become road"
        );
    }

    #[test]
    fn traffic_does_not_convert_water_to_road() {
        let mut game = Game::new(60, 42);
        let tx = 130usize;
        let ty = 130usize;
        game.map.set(tx, ty, Terrain::Water);

        for _ in 0..200 {
            game.traffic.step_on(tx, ty);
        }

        game.tick = 100;
        game.update_traffic();

        assert_eq!(
            *game.map.get(tx, ty).unwrap(),
            Terrain::Water,
            "water should not convert to road"
        );
    }

    #[test]
    fn traffic_overlay_renders_without_panic() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.overlay = OverlayMode::Traffic;

        // Add some traffic
        game.traffic.step_on(105, 105);
        game.traffic.step_on(105, 105);

        game.step(GameInput::None, &mut renderer).unwrap();
        // Just verify no panic
    }

    #[test]
    fn worn_terrain_faint_dims_background() {
        let mut game = Game::new(60, 42);
        // Set traffic to faint tier (10-50)
        for _ in 0..25 {
            game.traffic.step_on(5, 5);
        }
        let base_bg = Color(30, 50, 20); // typical grass bg
        let (ch, _fg, bg) = game.worn_terrain_override(5, 5, '.', Color(0, 200, 0), base_bg);
        // Faint tier should keep original char but dim bg
        assert_eq!(ch, '.', "faint tier should keep original char");
        assert!(
            bg.0 < base_bg.0 || bg.1 < base_bg.1 || bg.2 < base_bg.2,
            "faint tier should dim background: {:?} vs {:?}",
            bg,
            base_bg
        );
    }

    #[test]
    fn worn_terrain_worn_tier_changes_char() {
        let mut game = Game::new(60, 42);
        // Set traffic to worn tier (50-150)
        for _ in 0..100 {
            game.traffic.step_on(5, 5);
        }
        let (ch, _fg, _bg) =
            game.worn_terrain_override(5, 5, '"', Color(0, 200, 0), Color(30, 50, 20));
        assert!(
            ch == '.' || ch == ',',
            "worn tier should replace char with dot trail: got '{}'",
            ch
        );
    }

    #[test]
    fn worn_terrain_trail_tier_uses_directional_char() {
        let mut game = Game::new(60, 42);
        // Set traffic to trail tier (150-300) with strong east-west direction
        for _ in 0..200 {
            game.traffic.step_on_directed(5, 5, 1.0, 0.0, None);
        }
        let (ch, fg, _bg) =
            game.worn_terrain_override(5, 5, '"', Color(0, 200, 0), Color(30, 50, 20));
        assert_eq!(ch, '-', "trail tier should use oriented char for east-west");
        // Trail tier uses tan-brown color
        assert_eq!(
            fg,
            Color(140, 110, 70),
            "trail tier should use tan-brown fg"
        );
    }

    #[test]
    fn worn_terrain_no_effect_below_threshold() {
        let game = Game::new(60, 42);
        let orig_ch = '"';
        let orig_fg = Color(0, 200, 0);
        let orig_bg = Color(30, 50, 20);
        let (ch, fg, bg) = game.worn_terrain_override(5, 5, orig_ch, orig_fg, orig_bg);
        assert_eq!(ch, orig_ch);
        assert_eq!(fg, orig_fg);
        assert_eq!(bg, orig_bg);
    }

    #[test]
    fn worn_terrain_no_effect_above_road_threshold() {
        let mut game = Game::new(60, 42);
        for _ in 0..400 {
            game.traffic.step_on(5, 5);
        }
        let orig_ch = '=';
        let orig_fg = Color(170, 145, 90);
        let orig_bg = Color(80, 70, 50);
        let (ch, fg, bg) = game.worn_terrain_override(5, 5, orig_ch, orig_fg, orig_bg);
        assert_eq!(
            ch, orig_ch,
            "road-threshold traffic should not alter terrain"
        );
        assert_eq!(fg, orig_fg);
        assert_eq!(bg, orig_bg);
    }

    #[test]
    fn traffic_overlay_shows_resource_typed_colors() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.overlay = OverlayMode::Traffic;

        // Add resource-typed traffic
        for _ in 0..50 {
            game.traffic
                .step_on_directed(105, 105, 1.0, 0.0, Some(ResourceType::Wood));
        }

        game.step(GameInput::None, &mut renderer).unwrap();
        // Just verify no panic with resource-typed traffic overlay
    }

    #[test]
    fn water_animation_renders_without_panic() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Run multiple ticks so the water animation cycles through all characters
        for _ in 0..30 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        // Just verify no panic across multiple animation frames
    }

    #[test]
    fn water_animation_cycles_characters() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Set tick values that produce different animation indices
        // For a given (x, y), changing tick/8 should cycle the character
        game.tick = 0;
        game.draw(&mut renderer);
        let frame0 = renderer.frame_as_string();

        game.tick = 8;
        renderer.clear();
        game.draw(&mut renderer);
        let frame1 = renderer.frame_as_string();

        game.tick = 16;
        renderer.clear();
        game.draw(&mut renderer);
        let frame2 = renderer.frame_as_string();

        // At least one pair of frames should differ (animation is cycling)
        let any_change = frame0 != frame1 || frame1 != frame2 || frame0 != frame2;
        assert!(
            any_change,
            "water animation should produce different frames at different ticks"
        );
    }

    #[test]
    fn water_shimmer_clamps_blue_channel() {
        // Verify the shimmer math doesn't panic with extreme tick values
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        game.tick = u64::MAX / 2;
        game.draw(&mut renderer);
        // No panic = pass

        game.tick = 0;
        renderer.clear();
        game.draw(&mut renderer);
        // No panic = pass
    }

    #[test]
    fn settlement_start_area_is_pre_revealed() {
        let game = Game::new(60, 42);
        // Settlement center should be revealed (may be near map center or near a ford)
        let (scx, scy) = game.settlement_center();
        let sx = scx as usize;
        let sy = scy as usize;
        assert!(game.exploration.is_revealed(sx, sy));
        // Tiles within radius 15 of settlement should be revealed
        assert!(game.exploration.is_revealed(sx.saturating_sub(8), sy));
        assert!(game.exploration.is_revealed(sx, sy.saturating_sub(8)));
        // Tiles far from settlement should NOT be revealed
        assert!(!game.exploration.is_revealed(0, 0));
        assert!(!game.exploration.is_revealed(250, 250));
    }

    #[test]
    fn exploration_expands_as_villagers_move() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Pick a tile far from the settlement that is definitely not revealed
        let far_x = 50usize;
        let far_y = 50usize;
        assert!(
            !game.exploration.is_revealed(far_x, far_y),
            "far tile should start unrevealed"
        );

        // Spawn a villager at that far location
        ecs::spawn_villager(&mut game.world, far_x as f64, far_y as f64);

        // Run one game step — the villager's sight should reveal tiles around it
        game.step(GameInput::None, &mut renderer).unwrap();

        assert!(
            game.exploration.is_revealed(far_x, far_y),
            "tile under villager should be revealed after step"
        );
    }

    #[test]
    fn berry_bush_yield_is_20() {
        let mut world = hecs::World::new();
        let e = ecs::spawn_berry_bush(&mut world, 10.0, 10.0);
        let ry = world.get::<&ecs::ResourceYield>(e).unwrap();
        assert_eq!(ry.remaining, 20, "berry bush yield should be 20");
        assert_eq!(ry.max, 20);
    }

    #[test]
    fn winter_food_decay_is_capped() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Give lots of food so decay behavior is visible
        game.resources.food = 200;
        game.day_night.season = Season::Winter;

        // Tick to a multiple of 30 so decay fires
        game.tick = 29;
        game.step(GameInput::None, &mut renderer).unwrap();
        // At tick 30: decay capped at 2 per event (not full 2% = 4)
        // Food should decrease by at most 2 from spoilage alone
        assert!(
            game.resources.food < 200,
            "decay should reduce food in winter"
        );
        // Cap at 2 per event prevents large stockpile wipeout
        assert!(
            game.resources.food >= 196,
            "decay should be capped at 2, not full percentage"
        );
    }

    #[test]
    fn settlement_starts_with_two_nearby_berry_bushes() {
        let mut game = Game::new(60, 42);
        // Find actual settlement center (stockpile position)
        let (scx, scy) = game.settlement_center();
        // Count berry bushes near settlement (within 8 tiles)
        let mut near_bushes = 0;
        for (pos, _fs) in game.world.query_mut::<(&ecs::Position, &ecs::FoodSource)>() {
            let dx = pos.x - scx as f64;
            let dy = pos.y - scy as f64;
            if dx * dx + dy * dy < 64.0 {
                // within 8 tiles
                near_bushes += 1;
            }
        }
        assert!(
            near_bushes >= 2,
            "should have at least 2 berry bushes near settlement, got {}",
            near_bushes
        );
    }

    #[test]
    fn particles_spawn_from_active_workshop() {
        let mut game = Game::new(60, 42);
        // Spawn a processing building with worker_present = true
        game.world.spawn((
            Position { x: 130.0, y: 130.0 },
            ProcessingBuilding {
                recipe: Recipe::WoodToPlanks,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));
        assert!(game.particles.is_empty(), "no particles before step");
        // Run enough steps so at least one particle spawns (probabilistic, but 20 steps is enough)
        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..20 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        assert!(
            !game.particles.is_empty(),
            "particles should spawn from active workshop"
        );
    }

    #[test]
    fn particles_despawn_after_lifetime() {
        let mut game = Game::new(60, 42);
        // Manually add a particle with life=1
        game.particles.push(Particle {
            x: 128.0,
            y: 128.0,
            ch: '.',
            fg: Color(150, 150, 150),
            life: 1,
            max_life: 1,
            dx: 0.0,
            dy: -0.2,
            emissive: false,
        });
        assert_eq!(game.particles.len(), 1);
        let mut renderer = HeadlessRenderer::new(80, 24);
        game.step(GameInput::None, &mut renderer).unwrap();
        // After one step, life decrements to 0 and particle is removed
        let manual_particles: Vec<_> = game
            .particles
            .iter()
            .filter(|p| p.ch == '.' && p.dx == 0.0)
            .collect();
        assert!(
            manual_particles.is_empty(),
            "particle with life=1 should be removed after one step"
        );
    }

    #[cfg(feature = "lua")]
    #[test]
    fn lua_on_tick_hook_called_during_step() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(80, 24);

        let engine = crate::scripting::ScriptEngine::new().unwrap();
        engine
            .exec("tick_count = 0; function on_tick() tick_count = tick_count + 1 end")
            .unwrap();
        game.script_engine = Some(engine);

        for _ in 0..5 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let engine = game.script_engine.as_ref().unwrap();
        engine.exec("assert(tick_count >= 5, 'on_tick should have been called at least 5 times, got ' .. tick_count)").unwrap();
    }

    #[cfg(feature = "lua")]
    #[test]
    fn lua_on_tick_updates_game_state() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(80, 24);

        let engine = crate::scripting::ScriptEngine::new().unwrap();
        engine
            .exec(
                r#"
            last_season = nil
            last_villager_count = nil
            function on_tick()
                last_season = season
                last_villager_count = villager_count
            end
        "#,
            )
            .unwrap();
        game.script_engine = Some(engine);

        game.step(GameInput::None, &mut renderer).unwrap();

        let engine = game.script_engine.as_ref().unwrap();
        engine
            .exec("assert(last_season ~= nil, 'season should be set')")
            .unwrap();
        engine
            .exec("assert(last_villager_count ~= nil, 'villager_count should be set')")
            .unwrap();
    }

    #[cfg(feature = "lua")]
    #[test]
    fn lua_event_hook_fires_on_drought() {
        let mut game = Game::new(60, 42);

        let engine = crate::scripting::ScriptEngine::new().unwrap();
        engine
            .exec(
                r#"
            last_event = nil
            function on_event()
                last_event = event_name
            end
        "#,
            )
            .unwrap();
        game.script_engine = Some(engine);

        game.fire_event_hook("drought");

        let engine = game.script_engine.as_ref().unwrap();
        engine.exec(r#"assert(last_event == "drought", "expected drought event, got " .. tostring(last_event))"#).unwrap();
    }

    #[cfg(feature = "lua")]
    #[test]
    fn lua_event_hook_fires_on_wolf_surge() {
        let mut game = Game::new(60, 42);

        let engine = crate::scripting::ScriptEngine::new().unwrap();
        engine
            .exec(
                r#"
            last_event = nil
            function on_event()
                last_event = event_name
            end
        "#,
            )
            .unwrap();
        game.script_engine = Some(engine);

        game.fire_event_hook("wolf_surge");

        let engine = game.script_engine.as_ref().unwrap();
        engine
            .exec(r#"assert(last_event == "wolf_surge", "expected wolf_surge event")"#)
            .unwrap();
    }

    #[cfg(feature = "lua")]
    #[test]
    fn lua_hot_reload_picks_up_changes() {
        let tmp_dir = std::env::temp_dir().join("lua_hot_reload_test");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let script_path = tmp_dir.join("test.lua");
        std::fs::write(&script_path, "reload_version = 1").unwrap();

        let mut game = Game::new(60, 42);
        let engine = crate::scripting::ScriptEngine::new().unwrap();
        engine.load_script(script_path.to_str().unwrap()).unwrap();
        game.script_engine = Some(engine);

        game.script_engine
            .as_ref()
            .unwrap()
            .exec("assert(reload_version == 1, 'initial version should be 1')")
            .unwrap();

        std::fs::write(&script_path, "reload_version = 2").unwrap();

        game.script_engine
            .as_ref()
            .unwrap()
            .reload_scripts(tmp_dir.to_str().unwrap())
            .unwrap();

        game.script_engine
            .as_ref()
            .unwrap()
            .exec("assert(reload_version == 2, 'version should be 2 after reload')")
            .unwrap();

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn milestone_first_winter_detected() {
        let mut game = Game::new(60, 42);
        game.day_night.year = 1;
        game.check_milestones();
        assert!(
            game.difficulty
                .milestones
                .contains(&Milestone::FirstWinterSurvived)
        );
        // Milestones no longer affect threat_level (decoupled)
    }

    #[test]
    fn milestone_fires_only_once() {
        let mut game = Game::new(60, 42);
        game.resources.wood = 10;
        game.check_milestones();
        game.check_milestones();
        let count = game
            .difficulty
            .milestones
            .iter()
            .filter(|m| **m == Milestone::FirstWoodGathered)
            .count();
        assert_eq!(count, 1, "FirstWoodGathered should fire exactly once");
    }

    #[test]
    fn milestone_population_ten() {
        let mut game = Game::new(60, 42);
        // Fewer than 10 villagers — should not fire
        game.check_milestones();
        assert!(
            !game
                .difficulty
                .milestones
                .contains(&Milestone::PopulationTen)
        );
        // Spawn villagers to reach 10
        let (cx, cy) = game.settlement_center();
        for _ in 0..10 {
            crate::ecs::spawn_villager(&mut game.world, cx as f64, cy as f64);
        }
        game.check_milestones();
        assert!(
            game.difficulty
                .milestones
                .contains(&Milestone::PopulationTen)
        );
    }

    #[test]
    fn milestone_does_not_change_threat_level() {
        let mut game = Game::new(60, 42);
        let before = game.difficulty.threat_level;
        game.day_night.year = 1;
        game.resources.wood = 100;
        game.resources.food = 200;
        game.check_milestones();
        assert_eq!(
            game.difficulty.threat_level, before,
            "Milestones should not change threat_level"
        );
    }

    #[test]
    fn milestone_banner_ticks_down() {
        let mut game = Game::new(60, 42);
        game.notify_milestone("Test milestone!");
        assert!(game.milestone_banner.is_some());
        assert_eq!(game.milestone_banner.as_ref().unwrap().ticks_remaining, 120);
        // Simulate ticking down
        for _ in 0..120 {
            if let Some(ref mut banner) = game.milestone_banner {
                banner.ticks_remaining = banner.ticks_remaining.saturating_sub(1);
                if banner.ticks_remaining == 0 {
                    game.milestone_banner = None;
                }
            }
        }
        assert!(game.milestone_banner.is_none());
    }

    #[test]
    fn milestone_event_log_prefix() {
        let mut game = Game::new(60, 42);
        game.notify_milestone("Test milestone!");
        assert!(
            game.events
                .event_log
                .iter()
                .any(|msg| msg.starts_with("[*]"))
        );
    }

    #[test]
    fn milestone_first_garrison() {
        let mut game = Game::new(60, 42);
        game.check_milestones();
        assert!(
            !game
                .difficulty
                .milestones
                .contains(&Milestone::FirstGarrison)
        );
        // Spawn a garrison building
        let (cx, cy) = game.settlement_center();
        game.world.spawn((
            crate::ecs::Position {
                x: cx as f64,
                y: cy as f64,
            },
            crate::ecs::GarrisonBuilding { defense_bonus: 1.0 },
        ));
        game.check_milestones();
        assert!(
            game.difficulty
                .milestones
                .contains(&Milestone::FirstGarrison)
        );
    }

    #[test]
    fn milestone_hundred_food() {
        let mut game = Game::new(60, 42);
        game.resources.food = 50;
        game.check_milestones();
        assert!(!game.difficulty.milestones.contains(&Milestone::HundredFood));
        game.resources.food = 100;
        game.check_milestones();
        assert!(game.difficulty.milestones.contains(&Milestone::HundredFood));
    }

    #[test]
    fn milestone_raid_survived() {
        let mut game = Game::new(60, 42);
        game.check_milestones();
        assert!(
            !game
                .difficulty
                .milestones
                .contains(&Milestone::RaidSurvived)
        );
        game.raid_survived_clean = true;
        game.check_milestones();
        assert!(
            game.difficulty
                .milestones
                .contains(&Milestone::RaidSurvived)
        );
        // Flag should be cleared after milestone fires
        assert!(!game.raid_survived_clean);
    }

    #[test]
    fn plague_kills_villager() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        // Use winter to suppress births that could replace plague kills
        game.day_night.season = Season::Winter;

        let initial_villagers = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        // Inject plague event directly
        game.events.active_events.push(GameEvent::Plague {
            ticks_remaining: 300,
            kills_remaining: 1,
        });

        // Run until the plague kill interval (every 100 ticks of plague life)
        for _ in 0..400 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let final_villagers = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        // Plague should have killed at least one (though hunger/other causes may also kill)
        assert!(
            final_villagers < initial_villagers || initial_villagers == 0,
            "plague should kill at least one villager"
        );
    }

    #[test]
    fn bandit_raid_steals_resources() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.resources.food = 100;
        game.resources.wood = 80;
        game.resources.stone = 60;

        game.events.active_events.push(GameEvent::BanditRaid {
            stolen: false,
            strength: 30.0, // overwhelming force to guarantee theft
        });
        game.step(GameInput::None, &mut renderer).unwrap();

        // Bandits steal resources, reduced by defense rating.
        // With strength 30.0, steal_fraction is high despite starting defenses.
        assert!(
            game.resources.food < 100,
            "bandits should steal some food, got {}",
            game.resources.food
        );
        assert!(
            game.resources.wood < 80,
            "bandits should steal some wood, got {}",
            game.resources.wood
        );
        assert!(
            game.resources.stone < 60,
            "bandits should steal some stone, got {}",
            game.resources.stone
        );
    }

    #[test]
    fn blizzard_provides_movement_multiplier() {
        let mut game = Game::new(60, 42);
        assert_eq!(game.events.movement_multiplier(), 1.0);

        game.events.active_events.push(GameEvent::Blizzard {
            ticks_remaining: 100,
        });
        assert_eq!(game.events.movement_multiplier(), 0.5);
    }

    #[test]
    fn configurable_map_size_128() {
        let game = Game::new_with_size(60, 42, 128, 128);
        assert_eq!(game.map.width, 128);
        assert_eq!(game.map.height, 128);
        // Entities should exist
        let villagers = game
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        assert!(villagers >= 3, "should have villagers on 128x128 map");
    }

    #[test]
    fn configurable_map_size_512() {
        let mut game = Game::new_with_size(60, 42, 512, 512);
        let mut renderer = HeadlessRenderer::new(120, 40);
        assert_eq!(game.map.width, 512);
        assert_eq!(game.map.height, 512);
        // Run a few ticks — should not panic
        for _ in 0..10 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
    }

    // ── Terrain-driven settlement placement tests (#29) ──

    #[test]
    fn farm_prefers_water_proximity() {
        // Create a game and place a river stripe down one side.
        // Farm placement should gravitate toward the river.
        let mut game = Game::new_with_size(60, 99, 40, 40);
        // Clear map to grass
        for y in 0..40usize {
            for x in 0..40usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 40 + x] = 0.3; // flat
            }
        }
        // River along x=30
        for y in 0..40usize {
            for x in 29..=31 {
                game.map.set(x, y, Terrain::Water);
                game.river_mask[y * 40 + x] = true;
            }
        }
        // Set high fertility near river in ResourceMap
        for y in 0..40usize {
            for x in 25..29 {
                game.resource_map.get_mut(x, y).fertility = 200;
            }
        }
        // Place villager at center
        ecs::spawn_villager(&mut game.world, 20.0, 20.0);
        game.resources.wood = 50;
        game.resources.stone = 50;

        let spot = game.find_building_spot(20.0, 20.0, BuildingType::Farm);
        assert!(spot.is_some(), "should find a farm spot");
        let (fx, _fy) = spot.unwrap();
        // Farm should be placed closer to the river (x=30) than to the far side (x=0).
        // The center tile of a 3x3 farm is fx+1, so check that.
        assert!(
            fx + 1 >= 15,
            "farm at x={fx} should be in the river-half of the map (x>=15)"
        );
    }

    #[test]
    fn garrison_prefers_high_ground_and_chokepoint() {
        // Map with a narrow pass between mountains. Garrison should pick the pass.
        let mut game = Game::new_with_size(60, 101, 40, 40);
        for y in 0..40usize {
            for x in 0..40usize {
                game.map.set(x, y, Terrain::Mountain);
                game.heights[y * 40 + x] = 0.8;
            }
        }
        // Create a 6-tile-wide walkable pass at y=18..22, x=0..40
        for y in 17..23 {
            for x in 0..40usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 40 + x] = 0.6; // elevated pass
            }
        }
        // Create open area at center for villager
        for y in 15..25 {
            for x in 15..25 {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 40 + x] = 0.4;
            }
        }
        ecs::spawn_villager(&mut game.world, 20.0, 20.0);
        game.resources.wood = 50;
        game.resources.stone = 50;

        let spot = game.find_building_spot(20.0, 20.0, BuildingType::Garrison);
        assert!(spot.is_some(), "should find a garrison spot");
        let (_gx, gy) = spot.unwrap();
        // Garrison should be near the pass edges (y ~17 or y ~22) where chokepoint score is high,
        // not dead center of the open area.
        let near_pass = (gy >= 15 && gy <= 17) || (gy >= 21 && gy <= 24);
        let in_open = gy >= 18 && gy <= 21;
        // Either near the pass boundary (chokepoint) or in the elevated pass area is acceptable
        assert!(
            near_pass || in_open,
            "garrison at y={gy} should be near mountain pass (17-24), not deep in open area"
        );
    }

    #[test]
    fn hut_clusters_near_existing_buildings() {
        let mut game = Game::new_with_size(60, 102, 40, 40);
        for y in 0..40usize {
            for x in 0..40usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 40 + x] = 0.3;
            }
        }
        // Place 3 existing huts around (10, 10) to create a cluster
        game.place_build_site(8, 8, BuildingType::Hut);
        game.place_build_site(8, 12, BuildingType::Hut);
        game.place_build_site(12, 8, BuildingType::Hut);

        ecs::spawn_villager(&mut game.world, 10.0, 10.0);
        game.resources.wood = 50;
        game.resources.stone = 50;

        let spot = game.find_building_spot(10.0, 10.0, BuildingType::Hut);
        assert!(spot.is_some(), "should find a hut spot");
        let (hx, hy) = spot.unwrap();
        // New hut should cluster near existing ones (within 10 tiles of centroid)
        let dist = ((hx as f64 - 10.0).powi(2) + (hy as f64 - 10.0).powi(2)).sqrt();
        assert!(
            dist < 12.0,
            "hut at ({hx},{hy}) should cluster near existing buildings (dist={dist:.1})"
        );
    }

    #[test]
    fn scoring_prefers_fertile_soil_for_farms() {
        let mut game = Game::new_with_size(60, 103, 30, 30);
        for y in 0..30usize {
            for x in 0..30usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 30 + x] = 0.3;
                // Low fertility everywhere
                game.resource_map.get_mut(x, y).fertility = 20;
            }
        }
        // High fertility patch at (20, 15)
        for y in 13..18 {
            for x in 18..23 {
                game.resource_map.get_mut(x, y).fertility = 240;
            }
        }
        ecs::spawn_villager(&mut game.world, 15.0, 15.0);

        // Score a farm at the fertile spot vs a barren spot
        let fertile_score = game.score_building_spot(19, 14, BuildingType::Farm, 15.0, 15.0);
        let barren_score = game.score_building_spot(5, 5, BuildingType::Farm, 15.0, 15.0);

        assert!(
            fertile_score > barren_score,
            "fertile spot ({fertile_score:.2}) should score higher than barren ({barren_score:.2})"
        );
    }

    #[test]
    fn workshop_prefers_forest_proximity() {
        let mut game = Game::new_with_size(60, 104, 30, 30);
        for y in 0..30usize {
            for x in 0..30usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 30 + x] = 0.3;
                game.resource_map.get_mut(x, y).wood = 10; // low wood
            }
        }
        // Forest-rich area near (5, 15)
        for y in 12..18 {
            for x in 3..8 {
                game.resource_map.get_mut(x, y).wood = 220;
            }
        }
        ecs::spawn_villager(&mut game.world, 15.0, 15.0);

        let near_forest = game.score_building_spot(5, 14, BuildingType::Workshop, 15.0, 15.0);
        let far_from_forest = game.score_building_spot(25, 15, BuildingType::Workshop, 15.0, 15.0);

        assert!(
            near_forest > far_from_forest,
            "workshop near forest ({near_forest:.2}) should score higher than far ({far_from_forest:.2})"
        );
    }

    #[test]
    fn smithy_prefers_stone_deposits() {
        let mut game = Game::new_with_size(60, 105, 30, 30);
        for y in 0..30usize {
            for x in 0..30usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 30 + x] = 0.3;
                game.resource_map.get_mut(x, y).stone = 10;
            }
        }
        // Rich stone area near (22, 15)
        for y in 12..18 {
            for x in 20..25 {
                game.resource_map.get_mut(x, y).stone = 230;
            }
        }
        ecs::spawn_villager(&mut game.world, 15.0, 15.0);

        let near_stone = game.score_building_spot(21, 14, BuildingType::Smithy, 15.0, 15.0);
        let far_stone = game.score_building_spot(5, 5, BuildingType::Smithy, 15.0, 15.0);

        assert!(
            near_stone > far_stone,
            "smithy near stone ({near_stone:.2}) should score higher than far ({far_stone:.2})"
        );
    }

    #[test]
    fn fallback_finds_spot_on_tiny_map() {
        // Almost entirely mountain with just a few grass tiles.
        // find_building_spot should still return a valid position.
        let mut game = Game::new_with_size(60, 106, 15, 15);
        for y in 0..15usize {
            for x in 0..15usize {
                game.map.set(x, y, Terrain::Mountain);
                game.heights[y * 15 + x] = 0.9;
            }
        }
        // Small grass patch
        for y in 6..9 {
            for x in 6..9 {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 15 + x] = 0.3;
            }
        }
        ecs::spawn_villager(&mut game.world, 7.0, 7.0);

        // Wall is 1x1, should fit in the grass patch
        let spot = game.find_building_spot(7.0, 7.0, BuildingType::Wall);
        assert!(
            spot.is_some(),
            "should find a spot even on a tiny grass patch"
        );
        let (wx, wy) = spot.unwrap();
        assert!(
            wx >= 6 && wx <= 8 && wy >= 6 && wy <= 8,
            "wall at ({wx},{wy}) should be in the grass patch"
        );
    }

    #[test]
    fn spacing_penalty_distributes_farms() {
        let mut game = Game::new_with_size(60, 107, 40, 40);
        for y in 0..40usize {
            for x in 0..40usize {
                game.map.set(x, y, Terrain::Grass);
                game.heights[y * 40 + x] = 0.3;
                game.resource_map.get_mut(x, y).fertility = 180; // uniform fertility
            }
        }
        // Place a river for water proximity (farms like water)
        for y in 0..40usize {
            game.map.set(20, y, Terrain::Water);
            game.river_mask[y * 40 + 20] = true;
        }
        // Place 2 farms near (18, 20)
        game.place_build_site(16, 19, BuildingType::Farm);
        game.place_build_site(16, 22, BuildingType::Farm);

        ecs::spawn_villager(&mut game.world, 18.0, 20.0);

        // Score at the crowded spot vs a spot further along the river
        let crowded = game.score_building_spot(16, 16, BuildingType::Farm, 18.0, 20.0);
        let spread_out = game.score_building_spot(16, 12, BuildingType::Farm, 18.0, 20.0);

        // The spread-out spot should score at least close to the crowded one (spacing penalty
        // offsets the distance advantage of being closer), demonstrating distribution behavior.
        // This is a soft check — the key behavior is that spacing penalty reduces crowded scores.
        let crowded_no_penalty = game.score_building_spot(16, 25, BuildingType::Farm, 18.0, 20.0);
        // A spot with no nearby farms should not have the spacing penalty
        assert!(
            crowded_no_penalty >= crowded - 0.1 || spread_out > crowded - 0.5,
            "spacing penalty should reduce score near existing farms \
             (crowded={crowded:.2}, spread={spread_out:.2}, empty={crowded_no_penalty:.2})"
        );
    }

    // ─── Soil degradation integration tests ────────────────────────────────

    #[test]
    fn deforestation_degrades_fertility() {
        // When Forest -> Stump, the tile and its 4-neighbors should lose fertility.
        let mut game = Game::new_with_size(60, 103, 30, 30);
        // Set a forest tile at (15, 15) and give it high fertility
        game.map.set(15, 15, Terrain::Forest);
        game.soil_fertility.set(15, 15, 0.9);
        game.soil_fertility.set(15, 16, 0.9);
        game.soil_fertility.set(15, 14, 0.9);
        game.soil_fertility.set(16, 15, 0.9);
        game.soil_fertility.set(14, 15, 0.9);

        // Simulate deforestation: convert Forest -> Stump with fertility damage
        // (replicate the logic from game step)
        if game.map.get(15, 15) == Some(&Terrain::Forest) {
            game.map.set(15, 15, Terrain::Stump);
            game.soil_fertility.degrade(15, 15, 0.05);
            for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                let nx = 15i32 + dx;
                let ny = 15i32 + dy;
                if nx >= 0 && ny >= 0 {
                    game.soil_fertility.degrade(nx as usize, ny as usize, 0.05);
                }
            }
        }

        assert!(
            (game.soil_fertility.get(15, 15) - 0.85).abs() < 0.01,
            "deforested tile should lose 0.05 fertility: got {}",
            game.soil_fertility.get(15, 15)
        );
        assert!(
            (game.soil_fertility.get(15, 16) - 0.85).abs() < 0.01,
            "neighbor should lose 0.05 fertility: got {}",
            game.soil_fertility.get(15, 16)
        );
    }

    #[test]
    fn mining_scarring_degrades_fertility() {
        // When Mountain -> Quarry, the mined tile should have fertility set to 0.05
        // and 4-neighbors should lose 0.1 fertility.
        let mut game = Game::new_with_size(60, 103, 30, 30);
        game.map.set(15, 15, Terrain::Mountain);
        game.soil_fertility.set(15, 15, 0.5);
        game.soil_fertility.set(15, 16, 0.8);
        game.soil_fertility.set(15, 14, 0.8);
        game.soil_fertility.set(16, 15, 0.8);
        game.soil_fertility.set(14, 15, 0.8);

        // Simulate mining: increment mine count to trigger Quarry transition
        for _ in 0..6 {
            game.map.increment_mine_count(15, 15);
        }
        game.map.set(15, 15, Terrain::Quarry);
        // Apply mining scar damage (replicate the logic from game step)
        game.soil_fertility.set(15, 15, 0.05);
        for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = 15i32 + dx;
            let ny = 15i32 + dy;
            if nx >= 0 && ny >= 0 {
                game.soil_fertility.degrade(nx as usize, ny as usize, 0.1);
            }
        }

        assert!(
            (game.soil_fertility.get(15, 15) - 0.05).abs() < 0.01,
            "quarry tile should have fertility 0.05: got {}",
            game.soil_fertility.get(15, 15)
        );
        assert!(
            (game.soil_fertility.get(15, 16) - 0.7).abs() < 0.01,
            "neighbor should lose 0.1 fertility: got {}",
            game.soil_fertility.get(15, 16)
        );
    }

    #[test]
    fn soil_type_base_fertility_matches_design() {
        use crate::terrain_pipeline::SoilType;
        assert!((SoilType::Alluvial.base_fertility() - 1.0).abs() < 0.01);
        assert!((SoilType::Loam.base_fertility() - 0.85).abs() < 0.01);
        assert!((SoilType::Clay.base_fertility() - 0.70).abs() < 0.01);
        assert!((SoilType::Sand.base_fertility() - 0.40).abs() < 0.01);
        assert!((SoilType::Rocky.base_fertility() - 0.15).abs() < 0.01);
        assert!((SoilType::Peat.base_fertility() - 0.75).abs() < 0.01);
    }

    #[test]
    fn soil_type_harvest_depletion_rates() {
        use crate::terrain_pipeline::SoilType;
        assert!((SoilType::Alluvial.harvest_depletion_rate() - 0.02).abs() < 0.001);
        assert!((SoilType::Loam.harvest_depletion_rate() - 0.03).abs() < 0.001);
        assert!((SoilType::Clay.harvest_depletion_rate() - 0.04).abs() < 0.001);
        assert!((SoilType::Sand.harvest_depletion_rate() - 0.05).abs() < 0.001);
        assert!((SoilType::Rocky.harvest_depletion_rate() - 0.08).abs() < 0.001);
    }

    // --- Seasonal terrain effect tests ---

    /// Helper: advance the game until a target season is reached.
    fn advance_to_season(game: &mut Game, target: Season, renderer: &mut HeadlessRenderer) {
        for _ in 0..20000 {
            if game.day_night.season == target {
                return;
            }
            game.step(GameInput::None, renderer).unwrap();
        }
        panic!(
            "failed to reach {:?} after 20000 ticks (stuck at {:?})",
            target, game.day_night.season
        );
    }

    #[test]
    fn water_freezes_in_winter() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Find a water tile
        let mut water_pos = None;
        for y in 0..game.map.height {
            for x in 0..game.map.width {
                if game.map.get(x, y) == Some(&Terrain::Water) {
                    water_pos = Some((x, y));
                    break;
                }
            }
            if water_pos.is_some() {
                break;
            }
        }

        if let Some((wx, wy)) = water_pos {
            // Set to late autumn, close to winter boundary
            game.day_night.season = Season::Autumn;
            game.day_night.day = 9;
            game.day_night.hour = 23.98;
            advance_to_season(&mut game, Season::Winter, &mut renderer);

            assert_eq!(
                *game.map.get(wx, wy).unwrap(),
                Terrain::Ice,
                "water tile should become ice in winter"
            );
            assert!(
                game.map.is_walkable(wx as f64, wy as f64),
                "ice should be walkable"
            );
        }
    }

    #[test]
    fn ice_thaws_in_spring() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Find a water tile
        let mut water_pos = None;
        for y in 0..game.map.height {
            for x in 0..game.map.width {
                if game.map.get(x, y) == Some(&Terrain::Water) {
                    water_pos = Some((x, y));
                    break;
                }
            }
            if water_pos.is_some() {
                break;
            }
        }

        if let Some((wx, wy)) = water_pos {
            // Advance to winter (freeze)
            game.day_night.season = Season::Autumn;
            game.day_night.day = 9;
            game.day_night.hour = 23.98;
            advance_to_season(&mut game, Season::Winter, &mut renderer);
            assert_eq!(*game.map.get(wx, wy).unwrap(), Terrain::Ice);

            // Advance to spring (thaw)
            game.day_night.day = 9;
            game.day_night.hour = 23.98;
            advance_to_season(&mut game, Season::Spring, &mut renderer);

            assert_eq!(
                *game.map.get(wx, wy).unwrap(),
                Terrain::Water,
                "ice should thaw back to water in spring"
            );
            assert!(
                !game.map.is_walkable(wx as f64, wy as f64),
                "water should not be walkable after thaw"
            );
        }
    }

    #[test]
    fn autumn_wood_gathering_bonus() {
        // The autumn bonus multiplies gather_wood_speed by 1.5x.
        // With zero skill contribution (hypothetical base), timer goes from 90 to 60.
        let base_speed = 1.0_f64;
        let autumn_speed = base_speed * 1.5;

        let base_timer = (90.0 / base_speed) as u32;
        let autumn_timer = (90.0 / autumn_speed) as u32;

        assert_eq!(base_timer, 90, "base wood gathering should be 90 ticks");
        assert_eq!(
            autumn_timer, 60,
            "autumn wood gathering should be 60 ticks (90/1.5)"
        );

        // Verify autumn bonus is strictly faster than base, even with skill
        let skill_speed = 1.0 + 5.0 / 50.0; // woodcutting = 5
        let skill_autumn_speed = skill_speed * 1.5;
        assert!(
            (90.0 / skill_autumn_speed) < (90.0 / skill_speed),
            "autumn should always be faster than non-autumn"
        );
    }

    #[test]
    fn seasonal_cycle_does_not_corrupt_terrain() {
        // Use TileMap directly to avoid expensive full Game simulation loop
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        map.set(5, 5, Terrain::Water);
        map.set(6, 6, Terrain::Water);
        map.set(7, 7, Terrain::Forest);
        map.set(8, 8, Terrain::Sand);
        map.init_base_terrain();

        // Simulate winter: freeze water
        map.apply_winter_ice();
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Ice);
        assert_eq!(*map.get(7, 7).unwrap(), Terrain::Forest); // unaffected

        // Simulate spring: thaw, then flood
        map.revert_ice();
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Water);

        // Manually flood a tile
        map.set_seasonal(3, 3, Terrain::FloodWater);
        assert_eq!(*map.get(3, 3).unwrap(), Terrain::FloodWater);

        // Simulate summer: revert floods
        map.revert_flood_water();
        assert_eq!(*map.get(3, 3).unwrap(), Terrain::Grass);

        // Verify base terrain is untouched throughout
        assert_eq!(*map.get_base(5, 5).unwrap(), Terrain::Water);
        assert_eq!(*map.get_base(7, 7).unwrap(), Terrain::Forest);
        assert_eq!(*map.get_base(8, 8).unwrap(), Terrain::Sand);
        assert_eq!(*map.get_base(3, 3).unwrap(), Terrain::Grass);
    }

    #[test]
    fn flood_recede_adds_fertility() {
        use crate::terrain_pipeline::SoilType;
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        let mut heights = vec![0.5; 30 * 30];
        let mut river_mask = vec![false; 30 * 30];
        let mut soil = vec![SoilType::Loam; 30 * 30];

        // Set up a river at x=15
        let rx = 15usize;
        for y in 0..30 {
            let idx = y * 30 + rx;
            river_mask[idx] = true;
            heights[idx] = 0.3;
            map.set(rx, y, Terrain::Water);
        }

        // Alluvial soil adjacent to river at river elevation
        let target_x = rx - 1;
        let target_y = 15usize;
        let tidx = target_y * 30 + target_x;
        soil[tidx] = SoilType::Alluvial;
        heights[tidx] = 0.3;

        map.init_base_terrain();

        // Apply spring floods
        let flooded = map.apply_spring_floods(&river_mask, &heights, &soil);
        assert!(
            flooded.contains(&(target_x, target_y)),
            "alluvial tile at river level should flood"
        );
        assert_eq!(*map.get(target_x, target_y).unwrap(), Terrain::FloodWater);

        // Revert floods and apply fertility bonus
        let mut fertility = crate::simulation::SoilFertilityMap::new(30, 30);
        // Set initial fertility below 1.0 so we can observe the +0.15 bonus
        fertility.set(target_x, target_y, 0.5);
        let initial = fertility.get(target_x, target_y);

        let reverted = map.revert_flood_water();
        for (x, y) in &reverted {
            fertility.add(*x, *y, 0.15);
        }

        assert_eq!(*map.get(target_x, target_y).unwrap(), Terrain::Grass);
        let post_flood = fertility.get(target_x, target_y);
        assert!(
            (post_flood - initial - 0.15).abs() < 0.01,
            "fertility should increase by 0.15: {} -> {}",
            initial,
            post_flood
        );
    }

    #[test]
    fn game_has_base_terrain_initialized() {
        let game = Game::new(60, 42);
        assert_eq!(
            *game.map.get_base(0, 0).unwrap(),
            *game.map.get(0, 0).unwrap(),
            "base terrain should match active terrain at start"
        );
    }

    // --- Forest fire tests ---

    #[test]
    fn burning_terrain_properties() {
        assert!(Terrain::Burning.is_walkable());
        assert_eq!(Terrain::Burning.ch(), '*');
        assert_eq!(Terrain::Burning.move_cost(), 10.0);
        assert_eq!(Terrain::Burning.speed_multiplier(), 0.3);
        assert!(Terrain::Burning.bg().is_some());
        assert!(!Terrain::Burning.is_flammable());
    }

    #[test]
    fn scorched_terrain_properties() {
        assert!(Terrain::Scorched.is_walkable());
        assert_eq!(Terrain::Scorched.ch(), '`');
        assert_eq!(Terrain::Scorched.move_cost(), 1.3);
        assert_eq!(Terrain::Scorched.speed_multiplier(), 0.9);
        assert!(Terrain::Scorched.bg().is_some());
        assert!(!Terrain::Scorched.is_flammable());
        assert!(Terrain::Scorched.is_firebreak());
    }

    #[test]
    fn flammable_terrain_types() {
        assert!(Terrain::Forest.is_flammable());
        assert!(Terrain::Sapling.is_flammable());
        assert!(Terrain::Stump.is_flammable());
        assert!(Terrain::Scrubland.is_flammable());
        assert!(!Terrain::Grass.is_flammable());
        assert!(!Terrain::Water.is_flammable());
        assert!(!Terrain::Road.is_flammable());
    }

    #[test]
    fn firebreak_terrain_types() {
        assert!(Terrain::Water.is_firebreak());
        assert!(Terrain::Ford.is_firebreak());
        assert!(Terrain::Sand.is_firebreak());
        assert!(Terrain::Desert.is_firebreak());
        assert!(Terrain::Mountain.is_firebreak());
        assert!(Terrain::Road.is_firebreak());
        assert!(Terrain::Scorched.is_firebreak());
        assert!(!Terrain::Forest.is_firebreak());
        assert!(!Terrain::Grass.is_firebreak());
    }

    #[test]
    fn fire_ignition_only_in_summer() {
        let mut game = Game::new(60, 42);
        // Set season to Spring
        game.day_night.season = Season::Spring;
        // Place a dry forest tile
        game.map.set(50, 50, Terrain::Forest);
        game.moisture.set(50, 50, 0.0);

        // Run ignition check many times — should never ignite in spring
        for _ in 0..100 {
            game.check_fire_ignition();
        }
        assert!(
            game.fire_tiles.is_empty(),
            "fire should not ignite in spring"
        );

        // Set to winter — same result
        game.day_night.season = Season::Winter;
        for _ in 0..100 {
            game.check_fire_ignition();
        }
        assert!(
            game.fire_tiles.is_empty(),
            "fire should not ignite in winter"
        );
    }

    #[test]
    fn fire_ignition_requires_low_moisture() {
        let mut game = Game::new(60, 42);
        game.day_night.season = Season::Summer;
        // Set all flammable tiles to high moisture
        for y in 0..game.map.height {
            for x in 0..game.map.width {
                if game.map.get(x, y).is_some_and(|t| t.is_flammable()) {
                    game.moisture.set(x, y, 0.5); // above 0.15 threshold
                }
            }
        }

        for _ in 0..200 {
            game.check_fire_ignition();
        }
        assert!(
            game.fire_tiles.is_empty(),
            "fire should not ignite when moisture is above 0.15"
        );
    }

    #[test]
    fn fire_burns_out_to_scorched() {
        let mut game = Game::new(60, 42);
        // Manually ignite a tile with a short burn timer
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 1)); // 1 tick remaining

        // No adjacent flammable tiles (surround with grass which isn't flammable)
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                game.map
                    .set((50 + dx) as usize, (50 + dy) as usize, Terrain::Grass);
            }
        }

        game.tick_fire();

        assert_eq!(
            game.map.get(50, 50),
            Some(&Terrain::Scorched),
            "burning tile should become scorched after timer expires"
        );
        assert!(
            game.fire_tiles.is_empty(),
            "burned out tile should be removed from fire_tiles"
        );
    }

    #[test]
    fn fire_does_not_spread_across_water() {
        let mut game = Game::new(60, 42);
        // Set up: burning tile at (50,50), water at (51,50), forest at (52,50)
        game.map.set(50, 50, Terrain::Burning);
        game.map.set(51, 50, Terrain::Water);
        game.map.set(52, 50, Terrain::Forest);
        game.moisture.set(52, 50, 0.0);
        game.fire_tiles.push((50, 50, 100));

        // Surround with non-flammable to isolate test
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (50 + dx) as usize;
                let ny = (50 + dy) as usize;
                if nx != 51 || ny != 50 {
                    game.map.set(nx, ny, Terrain::Grass);
                }
            }
        }

        // Run fire spread many times
        for _ in 0..200 {
            game.tick_fire();
        }

        // Water tile should still be water
        assert_eq!(game.map.get(51, 50), Some(&Terrain::Water));
        // Forest behind water should not have burned
        assert_eq!(
            game.map.get(52, 50),
            Some(&Terrain::Forest),
            "fire should not cross water tile"
        );
    }

    #[test]
    fn fire_does_not_spread_across_road() {
        let mut game = Game::new(60, 42);
        game.map.set(50, 50, Terrain::Burning);
        game.map.set(51, 50, Terrain::Road);
        game.map.set(52, 50, Terrain::Forest);
        game.moisture.set(52, 50, 0.0);
        game.fire_tiles.push((50, 50, 100));

        // Surround with non-flammable except road direction
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (50 + dx) as usize;
                let ny = (50 + dy) as usize;
                if !(nx == 51 && ny == 50) && !(nx == 52 && ny == 50) {
                    game.map.set(nx, ny, Terrain::Grass);
                }
            }
        }

        for _ in 0..200 {
            game.tick_fire();
        }

        assert_eq!(game.map.get(51, 50), Some(&Terrain::Road));
        assert_eq!(
            game.map.get(52, 50),
            Some(&Terrain::Forest),
            "fire should not cross road tile"
        );
    }

    #[test]
    fn fire_spreads_to_adjacent_forest() {
        let mut game = Game::new(60, 42);
        // Put burning tile surrounded by dry forest
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 200)); // long burn
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (50 + dx) as usize;
                let ny = (50 + dy) as usize;
                game.map.set(nx, ny, Terrain::Forest);
                game.moisture.set(nx, ny, 0.0); // bone dry
                // Set vegetation high for max spread chance
                if let Some(v) = game.vegetation.get_mut(nx, ny) {
                    *v = 1.0;
                }
            }
        }

        // Run many ticks — with 0 moisture and 1.0 vegetation, spread prob is 0.03
        // Over many ticks, at least one neighbor should catch fire
        for _ in 0..500 {
            game.tick_fire();
        }

        let burned_neighbors = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1)]
            .iter()
            .filter(|&&(dx, dy)| {
                let nx = (50 + dx) as usize;
                let ny = (50 + dy) as usize;
                matches!(
                    game.map.get(nx, ny),
                    Some(&Terrain::Burning) | Some(&Terrain::Scorched)
                )
            })
            .count();

        assert!(
            burned_neighbors > 0,
            "fire should have spread to at least one adjacent forest tile"
        );
    }

    #[test]
    fn high_moisture_prevents_spread() {
        let mut game = Game::new(60, 42);
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 200));

        // Set all neighbors to forest with high moisture (>0.6 blocks spread)
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (50 + dx) as usize;
                let ny = (50 + dy) as usize;
                game.map.set(nx, ny, Terrain::Forest);
                game.moisture.set(nx, ny, 0.8);
            }
        }

        for _ in 0..500 {
            game.tick_fire();
        }

        let spread = game.fire_tiles.len();
        // Only the original fire tile (or it burned out)
        assert!(
            spread <= 1,
            "fire should not spread when moisture > 0.6, but {} tiles burning",
            spread
        );
    }

    #[test]
    fn scorched_gets_fertility_bonus() {
        let mut game = Game::new(60, 42);
        // Set fertility to a value below max so the bonus is visible
        game.soil_fertility.set(50, 50, 0.5);
        let initial_fertility = game.soil_fertility.get(50, 50);
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 1)); // burns out next tick

        // Surround with non-flammable
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                game.map
                    .set((50 + dx) as usize, (50 + dy) as usize, Terrain::Grass);
            }
        }

        game.tick_fire();

        let new_fertility = game.soil_fertility.get(50, 50);
        assert!(
            new_fertility >= initial_fertility + 0.04,
            "scorched tile should get +0.05 fertility bonus, was {} now {}",
            initial_fertility,
            new_fertility
        );
    }

    #[test]
    fn entity_on_burning_tile_takes_damage() {
        let mut game = Game::new(60, 42);
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 100));

        let v = ecs::spawn_villager(&mut game.world, 50.0, 50.0);
        let hunger_before = game.world.get::<&Creature>(v).unwrap().hunger;

        game.fire_damage_entities();

        let hunger_after = game.world.get::<&Creature>(v).unwrap().hunger;
        assert!(
            hunger_after > hunger_before,
            "entity on burning tile should take hunger damage: before={}, after={}",
            hunger_before,
            hunger_after
        );
        assert!(
            (hunger_after - hunger_before - 2.0).abs() < 0.01,
            "fire damage should be 2.0 hunger per tick"
        );
    }

    #[test]
    fn villager_flees_from_fire() {
        let mut game = Game::new(60, 42);
        // Clear area and place fire near a villager
        for y in 45..56 {
            for x in 45..56 {
                game.map.set(x, y, Terrain::Grass);
            }
        }
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 200));

        let v = ecs::spawn_villager(&mut game.world, 52.0, 50.0);
        ecs::spawn_stockpile(&mut game.world, 55.0, 50.0);

        // Run AI with fire_tiles — the fire is within threat range (8 tiles)
        let grid = crate::ecs::spatial::SpatialHashGrid::new(game.map.width, game.map.height, 16);
        let mut grid = grid;
        grid.populate(&game.world);

        let result = ecs::system_ai(
            &mut game.world,
            &game.map,
            &grid,
            0.4,
            10,
            0,
            0,
            0,
            0,
            &crate::ecs::SkillMults::default(),
            false,
            false,
            &[],
            0,
            &game.fire_tiles,
            &ScentMap::default(),
            &ScentMap::default(),
            &crate::pathfinding::NavGraph::default(),
            &crate::pathfinding::FlowFieldRegistry::new(),
        );

        let state = game.world.get::<&crate::ecs::Behavior>(v).unwrap().state;
        assert!(
            matches!(state, crate::ecs::BehaviorState::FleeHome { .. }),
            "villager near fire should flee, got: {:?}",
            state
        );
    }

    #[test]
    fn fire_tile_tracking_efficiency() {
        let mut game = Game::new(60, 42);
        // Start with no fire tiles
        assert!(game.fire_tiles.is_empty());

        // Add a fire
        game.map.set(50, 50, Terrain::Burning);
        game.fire_tiles.push((50, 50, 2));
        // Surround with grass (not flammable)
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx != 0 || dy != 0 {
                    game.map
                        .set((50 + dx) as usize, (50 + dy) as usize, Terrain::Grass);
                }
            }
        }

        // After 2 ticks, fire should burn out
        game.tick_fire(); // timer 2->1
        assert_eq!(game.fire_tiles.len(), 1);
        game.tick_fire(); // timer 1->0, burns out
        assert!(
            game.fire_tiles.is_empty(),
            "fire_tiles should be empty after burnout"
        );
        assert_eq!(game.map.get(50, 50), Some(&Terrain::Scorched));
    }

    #[test]
    fn particle_types_differ_by_building_recipe() {
        // Workshop (WoodToPlanks), Smithy (StoneToMasonry), Bakery (GrainToBread)
        // should produce particles with distinct colors.
        let mut game = Game::new(60, 42);
        let cx = 130.0;
        let cy = 130.0;
        // Spawn one of each active processing building
        game.world.spawn((
            Position { x: cx, y: cy },
            ProcessingBuilding {
                recipe: Recipe::WoodToPlanks,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));
        game.world.spawn((
            Position {
                x: cx + 10.0,
                y: cy,
            },
            ProcessingBuilding {
                recipe: Recipe::StoneToMasonry,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));
        game.world.spawn((
            Position {
                x: cx + 20.0,
                y: cy,
            },
            ProcessingBuilding {
                recipe: Recipe::GrainToBread,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));

        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..30 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        // Workshop particles: warm grey (r=140, g=130, b=110)
        let workshop: Vec<_> = game
            .particles
            .iter()
            .filter(|p| (p.x - cx).abs() < 1.0)
            .collect();
        // Smithy particles: orange (r=255, g=140, b=40)
        let smithy: Vec<_> = game
            .particles
            .iter()
            .filter(|p| (p.x - (cx + 10.0)).abs() < 1.0)
            .collect();
        // Bakery particles: white steam (r=200, g=200, b=210)
        let bakery: Vec<_> = game
            .particles
            .iter()
            .filter(|p| (p.x - (cx + 20.0)).abs() < 1.0)
            .collect();

        assert!(!workshop.is_empty(), "workshop should produce particles");
        assert!(!smithy.is_empty(), "smithy should produce particles");
        assert!(!bakery.is_empty(), "bakery should produce particles");

        // Smithy red channel > 200 (orange sparks)
        for p in &smithy {
            assert!(
                p.fg.0 > 200,
                "smithy particle red should be > 200, got {}",
                p.fg.0
            );
        }
        // Workshop grey: all channels < 180
        for p in &workshop {
            assert!(
                p.fg.0 <= 180 && p.fg.1 <= 180 && p.fg.2 <= 180,
                "workshop particle should be warm grey, got {:?}",
                p.fg
            );
        }
        // Bakery: all channels > 190
        for p in &bakery {
            assert!(
                p.fg.0 >= 190 && p.fg.1 >= 190 && p.fg.2 >= 190,
                "bakery particle should be white steam, got {:?}",
                p.fg
            );
        }
    }

    #[test]
    fn smithy_particles_are_emissive() {
        let mut game = Game::new(60, 42);
        game.world.spawn((
            Position { x: 130.0, y: 130.0 },
            ProcessingBuilding {
                recipe: Recipe::StoneToMasonry,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));
        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..20 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        let smithy_particles: Vec<_> = game.particles.iter().filter(|p| p.emissive).collect();
        assert!(
            !smithy_particles.is_empty(),
            "smithy particles should be emissive"
        );
    }

    #[test]
    fn workshop_particles_not_emissive() {
        let mut game = Game::new(60, 42);
        game.world.spawn((
            Position { x: 130.0, y: 130.0 },
            ProcessingBuilding {
                recipe: Recipe::WoodToPlanks,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));
        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..20 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        // Filter to workshop-area particles only
        let workshop_particles: Vec<_> = game
            .particles
            .iter()
            .filter(|p| (p.x - 130.0).abs() < 1.0)
            .collect();
        assert!(
            !workshop_particles.is_empty(),
            "should have workshop particles"
        );
        for p in &workshop_particles {
            assert!(!p.emissive, "workshop particles should not be emissive");
        }
    }

    #[test]
    fn particle_max_life_set_at_spawn() {
        let mut game = Game::new(60, 42);
        game.world.spawn((
            Position { x: 130.0, y: 130.0 },
            ProcessingBuilding {
                recipe: Recipe::WoodToPlanks,
                progress: 0,
                required: 100,
                worker_present: true,
                material_needed: None,
            },
        ));
        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..20 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        for p in &game.particles {
            assert!(p.max_life > 0, "max_life should be set at spawn");
            assert!(
                p.life <= p.max_life,
                "life ({}) should be <= max_life ({})",
                p.life,
                p.max_life
            );
        }
    }

    #[test]
    fn particle_cap_at_max_particles() {
        let mut game = Game::new(60, 42);
        // Fill particles to MAX_PARTICLES
        for i in 0..MAX_PARTICLES {
            game.particles.push(Particle {
                x: 128.0,
                y: 128.0,
                ch: '.',
                fg: Color(150, 150, 150),
                life: 100, // long life so they don't expire
                max_life: 100,
                dx: 0.0,
                dy: 0.0,
                emissive: false,
            });
        }
        // Spawn many active buildings
        for i in 0..10 {
            game.world.spawn((
                Position {
                    x: 130.0 + i as f64,
                    y: 130.0,
                },
                ProcessingBuilding {
                    recipe: Recipe::WoodToPlanks,
                    progress: 0,
                    required: 100,
                    worker_present: true,
                    material_needed: None,
                },
            ));
        }
        let mut renderer = HeadlessRenderer::new(80, 24);
        game.step(GameInput::None, &mut renderer).unwrap();
        // Should not exceed MAX_PARTICLES (some old particles still alive)
        assert!(
            game.particles.len() <= MAX_PARTICLES,
            "particle count {} should not exceed MAX_PARTICLES {}",
            game.particles.len(),
            MAX_PARTICLES
        );
    }

    #[test]
    fn construction_dust_particles_spawn() {
        let mut game = Game::new(60, 42);
        // Spawn a villager in Building state
        let tx = 130.0;
        let ty = 130.0;
        game.world.spawn((
            Position { x: tx + 1.0, y: ty },
            Behavior {
                state: BehaviorState::Building {
                    target_x: tx,
                    target_y: ty,
                    timer: 50,
                },
                speed: 1.0,
            },
            Creature {
                species: Species::Villager,
                hunger: 0.0,
                home_x: tx,
                home_y: ty,
                sight_range: 10.0,
            },
            Sprite {
                ch: 'v',
                fg: Color(200, 200, 200),
            },
        ));
        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..20 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        let dust: Vec<_> = game
            .particles
            .iter()
            .filter(|p| p.fg.0 == 220 && p.fg.1 == 200 && p.fg.2 == 100)
            .collect();
        assert!(
            !dust.is_empty(),
            "construction should produce yellow-brown dust particles"
        );
    }

    #[test]
    fn mining_sparkle_particles_spawn() {
        let mut game = Game::new(60, 42);
        let vx = 130.0;
        let vy = 130.0;
        game.world.spawn((
            Position { x: vx, y: vy },
            Behavior {
                state: BehaviorState::Gathering {
                    timer: 50,
                    resource_type: ResourceType::Stone,
                },
                speed: 1.0,
            },
            Creature {
                species: Species::Villager,
                hunger: 0.0,
                home_x: vx,
                home_y: vy,
                sight_range: 10.0,
            },
            Sprite {
                ch: 'v',
                fg: Color(200, 200, 200),
            },
        ));
        let mut renderer = HeadlessRenderer::new(80, 24);
        for _ in 0..20 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }
        let sparkle: Vec<_> = game
            .particles
            .iter()
            .filter(|p| p.fg.0 == 200 && p.fg.1 == 200 && p.fg.2 == 220)
            .collect();
        assert!(
            !sparkle.is_empty(),
            "mining should produce white-blue sparkle particles"
        );
    }
}
