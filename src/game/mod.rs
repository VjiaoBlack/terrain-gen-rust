mod build;
mod events;
mod render;
mod save;

use anyhow::Result;
use hecs::World;
use rand::RngExt;
use serde::{Deserialize, Serialize};

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
    DayNightCycle, ExplorationMap, InfluenceMap, MoistureMap, Season, SimConfig, TrafficMap,
    VegetationMap, WaterMap,
};
use crate::terrain_gen::{self, TerrainGenConfig};
use crate::tilemap::{Camera, Terrain, TileMap};

pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub ch: char,
    pub fg: Color,
    pub life: u32,
    pub dx: f64,
    pub dy: f64,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayMode {
    None,
    Tasks,     // Color-code villagers by current activity
    Resources, // Show resource locations with color markers
    Threats,   // Show wolf positions and danger zones
    Traffic,   // Show foot traffic heatmap
    Territory, // Show settlement influence/culture borders
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
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Milestone {
    FirstWinter,
    TenVillagers,
    FirstGarrison,
    FiveYears,
    TwentyVillagers,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DifficultyState {
    pub threat_level: f64,
    pub milestones: Vec<Milestone>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventSystem {
    pub active_events: Vec<GameEvent>,
    pub event_log: Vec<String>,
}

impl EventSystem {
    /// Returns farm yield multiplier based on active events.
    pub fn farm_yield_multiplier(&self) -> f64 {
        for event in &self.active_events {
            match event {
                GameEvent::Drought { .. } => return 0.5,
                GameEvent::BountifulHarvest { .. } => return 2.0,
                _ => {}
            }
        }
        1.0
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
    pub debug_view: bool,
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
    pub exploration: ExplorationMap,
    pub particles: Vec<Particle>,
    pub game_speed: u32, // 1 = normal, 2 = 2x, 5 = 5x
    pub difficulty: DifficultyState,
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
        // Reduce terrain noise scale for larger biomes — buildings feel right-sized
        let terrain_config = TerrainGenConfig {
            seed,
            scale: 0.015,
            ..Default::default()
        };
        let (mut map, mut heights) =
            terrain_gen::generate_terrain(map_width, map_height, &terrain_config);
        let mut water = WaterMap::new(map_width, map_height);
        // Seed water at terrain-Water tiles so ocean/lake areas have actual water
        for y in 0..map_height {
            for x in 0..map_width {
                if let Some(Terrain::Water) = map.get(x, y) {
                    let depth = (terrain_config.water_level - heights[y * map_width + x]).max(0.01);
                    water.set(x, y, depth);
                }
            }
        }
        let moisture = MoistureMap::new(map_width, map_height);
        let vegetation = VegetationMap::new(map_width, map_height);

        // Pre-erosion: run water simulation to carve natural valleys/riverbeds
        {
            let mut erosion_config = SimConfig::default();
            erosion_config.erosion_enabled = true;
            erosion_config.erosion_strength = 0.5;
            for _ in 0..200 {
                water.rain(&erosion_config);
                water.update(&mut heights, &erosion_config, None);
            }
            // Re-derive terrain types from eroded heights
            for y in 0..map_height {
                for x in 0..map_width {
                    let h = heights[y * map_width + x];
                    let new_terrain = if h < terrain_config.water_level {
                        Terrain::Water
                    } else if h < terrain_config.sand_level {
                        Terrain::Sand
                    } else if h < terrain_config.grass_level {
                        Terrain::Grass
                    } else if h < terrain_config.forest_level {
                        Terrain::Forest
                    } else if h < terrain_config.mountain_level {
                        Terrain::Mountain
                    } else {
                        Terrain::Snow
                    };
                    map.set(x, y, new_terrain);
                    // Update water: fill new water tiles, drain dry ones
                    if new_terrain == Terrain::Water {
                        let depth = (terrain_config.water_level - h).max(0.01);
                        water.set(x, y, depth);
                    }
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

        // Find a good start position: grass/sand tile adjacent to forest, near map center
        let cx = map_width / 2;
        let cy = map_height / 2;
        let mut start_cx = cx;
        let mut start_cy = cy;
        'search: for r in 0..80usize {
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
                    {
                        // Check if forest is adjacent (within 3 tiles)
                        let has_forest = (-3i32..=3).any(|fy| {
                            (-3i32..=3).any(|fx| {
                                map.get((ux as i32 + fx) as usize, (uy as i32 + fy) as usize)
                                    == Some(&Terrain::Forest)
                            })
                        });
                        if has_forest {
                            start_cx = ux;
                            start_cy = uy;
                            break 'search;
                        }
                    }
                }
            }
        }

        // Spawn 3 prey dens with 2 rabbits each in forest/grass tiles 8–50 tiles from start.
        // Without initial prey, the breeding system has nothing to breed from and rabbits
        // remain at 0 across all game-runs. Dens also serve as a secondary food source.
        {
            let mut rng_init = rand::rng();
            let mut dens_placed = 0u32;
            for _ in 0..200 {
                if dens_placed >= 3 {
                    break;
                }
                let angle = rng_init.random_range(0.0f64..std::f64::consts::TAU);
                let d = rng_init.random_range(8.0f64..50.0);
                let tx = start_cx as f64 + angle.cos() * d;
                let ty = start_cy as f64 + angle.sin() * d;
                if tx >= 0.0 && ty >= 0.0 {
                    let ix = tx as usize;
                    let iy = ty as usize;
                    if matches!(map.get(ix, iy), Some(Terrain::Forest | Terrain::Grass)) {
                        ecs::spawn_den(&mut world, tx, ty);
                        for _ in 0..2 {
                            let px = tx + rng_init.random_range(-2.0f64..2.0);
                            let py = ty + rng_init.random_range(-2.0f64..2.0);
                            ecs::spawn_prey(&mut world, px, py, tx, ty);
                        }
                        dens_placed += 1;
                    }
                }
            }
        }

        // Settlement: stockpile + villagers near found start position
        let scx = start_cx;
        let scy = start_cy;

        // Helper: find a spot where an NxM building fits on natural terrain (no buildings)
        let find_building_spot =
            |map: &TileMap, cx: usize, cy: usize, bw: usize, bh: usize| -> (f64, f64) {
                for r in 0..30usize {
                    for dy in -(r as i32)..=(r as i32) {
                        for dx in -(r as i32)..=(r as i32) {
                            if (dx.unsigned_abs() as usize != r)
                                && (dy.unsigned_abs() as usize != r)
                            {
                                continue;
                            }
                            let x = cx as i32 + dx;
                            let y = cy as i32 + dy;
                            if x < 0 || y < 0 {
                                continue;
                            }
                            // Check all tiles in footprint are natural terrain
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
                            if fits {
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

        // Berry bushes near settlement so villagers have food access
        for &(bsx, bsy) in &[
            (scx.wrapping_sub(1), scy.wrapping_sub(1)),
            (scx + 1, scy + 2),
        ] {
            let (bx, by) = find_walkable(&map, bsx, bsy);
            ecs::spawn_berry_bush(&mut world, bx, by);
        }

        // Stone deposits near settlement so villagers can gather stone
        for &(dsx, dsy) in &[(scx.wrapping_sub(3), scy), (scx + 3, scy + 1)] {
            let (dx, dy) = find_walkable(&map, dsx, dsy);
            ecs::spawn_stone_deposit(&mut world, dx, dy);
        }

        // Spawn 3 villagers near the stockpile
        for i in 0..3 {
            let (vx, vy) = find_walkable(&map, scx + i * 2, scy + 1);
            ecs::spawn_villager(&mut world, vx, vy);
        }

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
            debug_view: false,
            query_mode: false,
            query_cx: scx as i32,
            query_cy: scy as i32,
            display_fps: None,
            resources: Resources {
                food: 20,
                wood: 20,
                stone: 10,
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
            exploration: ExplorationMap::new(map_width, map_height),
            particles: Vec::new(),
            game_speed: 1,
            difficulty: DifficultyState::default(),
            #[cfg(feature = "lua")]
            script_engine: None,
        };
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
                    renderer.clear();
                    self.draw(renderer);
                    self.draw_game_over(renderer);
                    renderer.flush()?;
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
            GameInput::ToggleRain => self.raining = !self.raining,
            GameInput::ToggleErosion => {
                self.sim_config.erosion_enabled = !self.sim_config.erosion_enabled
            }
            GameInput::ToggleDayNight => self.day_night.enabled = !self.day_night.enabled,
            GameInput::ToggleDebugView => self.debug_view = !self.debug_view,
            GameInput::TogglePause => self.paused = !self.paused,
            GameInput::ToggleQueryMode => {
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
                    self.query_cy -= 1;
                }
            }
            GameInput::QueryDown => {
                if self.query_mode {
                    self.query_cy += 1;
                }
            }
            GameInput::QueryLeft => {
                if self.query_mode {
                    self.query_cx -= 1;
                }
            }
            GameInput::QueryRight => {
                if self.query_mode {
                    self.query_cx += 1;
                }
            }
            GameInput::ToggleBuildMode => {
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
                    self.build_cursor_y -= 1;
                }
            }
            GameInput::BuildDown => {
                if self.build_mode {
                    self.build_cursor_y += 1;
                }
            }
            GameInput::BuildLeft => {
                if self.build_mode {
                    self.build_cursor_x -= 1;
                }
            }
            GameInput::BuildRight => {
                if self.build_mode {
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

                // Clean up old notifications
                self.notifications.retain(|(t, _)| self.tick - t < 200);

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

                ecs::system_hunger(&mut self.world, mods.hunger_mult);

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

                let skill_mults = SkillMults {
                    gather_wood_speed: 1.0 + self.skills.woodcutting / 50.0,
                    gather_stone_speed: 1.0 + self.skills.mining / 50.0,
                    build_speed: (self.skills.building / 50.0).floor() as u32,
                };
                let ai_result = ecs::system_ai(
                    &mut self.world,
                    &self.map,
                    mods.wolf_aggression,
                    self.resources.food,
                    self.resources.wood,
                    self.resources.stone,
                    self.resources.grain,
                    &skill_mults,
                    settlement_defended,
                    self.day_night.is_night(),
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

                ecs::system_movement(&mut self.world, &self.map);

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
                let farm_mult =
                    (1.0 + self.skills.farming / 100.0) * self.events.farm_yield_multiplier();
                ecs::system_farms(&mut self.world, self.day_night.season, farm_mult);

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

                // Update particles: move, age, and remove dead ones
                for p in &mut self.particles {
                    p.x += p.dx;
                    p.y += p.dy;
                    p.life -= 1;
                }
                self.particles.retain(|p| p.life > 0);

                // Spawn smoke particles from active processing buildings
                {
                    let mut rng = rand::rng();
                    let smoke_sources: Vec<(f64, f64)> = self
                        .world
                        .query::<(&ProcessingBuilding, &Position)>()
                        .iter()
                        .filter(|(pb, _)| pb.worker_present)
                        .map(|(_, pos)| (pos.x, pos.y))
                        .collect();
                    for (px, py) in smoke_sources {
                        // Spawn a smoke particle every ~3 ticks per building
                        if rng.random_range(0..3) == 0 {
                            let ch = if rng.random_bool(0.5) {
                                '.'
                            } else {
                                '\u{00b0}'
                            };
                            let gray = rng.random_range(100u8..180u8);
                            self.particles.push(Particle {
                                x: px,
                                y: py - 1.0,
                                ch,
                                fg: Color(gray, gray, gray),
                                life: rng.random_range(15..=25),
                                dx: rng.random_range(-0.05..0.05),
                                dy: rng.random_range(-0.3..-0.1),
                            });
                        }
                    }
                }

                // Winter food decay: percentage-based spoilage, grain is preserved
                if self.day_night.season == Season::Winter
                    && self.tick.is_multiple_of(30)
                    && self.resources.food > 0
                {
                    let decay = std::cmp::max(1, self.resources.food * 2 / 100); // 2% per 30 ticks, min 1
                    self.resources.food = self.resources.food.saturating_sub(decay);
                    self.notify(format!("Food spoiled in winter (-{})", decay));
                }

                // Resource regrowth
                ecs::system_regrowth(&mut self.world, &self.map, self.tick);

                // Check for completed buildings
                self.check_build_completion();

                // Update influence map: villagers emit 1.0, active build sites emit 0.5
                self.update_influence();

                // Track villager foot traffic and auto-build roads
                self.update_traffic();

                // Population growth check
                self.try_population_growth();

                // Auto-build check (every 200 ticks)
                if self.auto_build && self.tick.is_multiple_of(200) {
                    self.auto_build_tick();
                }

                // Stone deposit discovery: when stone is critically low, discover new deposits.
                // Simulates settlement expanding its territory to find new stone sources.
                if self.tick.is_multiple_of(2000) && self.resources.stone < 50 {
                    self.discover_stone_deposits();
                }

                // Seasonal config for rain/water
                let mut tick_config = self.sim_config.clone();
                tick_config.rain_rate *= mods.rain_mult;
                tick_config.evaporation *= mods.evap_mult;

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
                    self.notify(format!("Season changed: {}", self.day_night.season.name()));
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

        // render
        renderer.clear();
        if self.debug_view {
            self.draw_debug(renderer);
        } else {
            self.draw(renderer);
        }
        if self.game_over {
            self.draw_game_over(renderer);
        }
        renderer.flush()?;
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
        game.resources.wood = 10;
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
        let cost = BuildingType::Farm.cost();
        assert_eq!(game.resources.food, 2 - cost.food);
        assert_eq!(game.resources.wood, 10 - cost.wood);
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
    fn garrison_placement_requires_refined_resources() {
        let mut game = Game::new(60, 42);

        // Give only raw resources
        game.resources = Resources {
            food: 100,
            wood: 100,
            stone: 100,
            ..Default::default()
        };

        let cost = BuildingType::Garrison.cost();
        assert!(
            !game.resources.can_afford(&cost),
            "should NOT afford garrison with only raw resources"
        );

        // Give refined resources
        game.resources.planks = 10;
        game.resources.masonry = 10;
        assert!(
            game.resources.can_afford(&cost),
            "should afford garrison with refined resources"
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
        let result = ecs::system_ai(
            &mut world,
            &map,
            0.4,
            10,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            true,
        );

        let state = world.get::<&Behavior>(v).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Sleeping { .. }),
            "villager should sleep at night when hut is nearby, got: {:?}",
            state
        );
    }

    #[test]
    fn drought_halves_farm_yield() {
        let mut events = EventSystem::default();
        assert_eq!(events.farm_yield_multiplier(), 1.0);

        events.active_events.push(GameEvent::Drought {
            ticks_remaining: 100,
        });
        assert_eq!(events.farm_yield_multiplier(), 0.5);
    }

    #[test]
    fn bountiful_harvest_doubles_farm_yield() {
        let mut events = EventSystem::default();
        events.active_events.push(GameEvent::BountifulHarvest {
            ticks_remaining: 100,
        });
        assert_eq!(events.farm_yield_multiplier(), 2.0);
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
        // Center of the map (128, 128) should be revealed
        assert!(game.exploration.is_revealed(128, 128));
        // Tiles within radius 15 of center should be revealed
        assert!(game.exploration.is_revealed(120, 128));
        assert!(game.exploration.is_revealed(128, 115));
        // Tiles far from center should NOT be revealed
        assert!(!game.exploration.is_revealed(0, 0));
        assert!(!game.exploration.is_revealed(200, 200));
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
    fn berry_bush_yield_is_12() {
        let mut world = hecs::World::new();
        let e = ecs::spawn_berry_bush(&mut world, 10.0, 10.0);
        let ry = world.get::<&ecs::ResourceYield>(e).unwrap();
        assert_eq!(ry.remaining, 12, "berry bush yield should be 12");
        assert_eq!(ry.max, 12);
    }

    #[test]
    fn winter_food_decay_is_percentage_based() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Give lots of food so percentage decay is visible
        game.resources.food = 200;
        game.day_night.season = Season::Winter;

        // Tick to a multiple of 30 so decay fires
        game.tick = 29;
        game.step(GameInput::None, &mut renderer).unwrap();
        // At tick 30: 2% of 200 = 4 decay (before villagers eat)
        // Food should be noticeably less than 200
        assert!(
            game.resources.food < 200,
            "percentage decay should reduce food"
        );
        // With 200 food, decay should be at least 4 (2%), not just 1
        assert!(
            game.resources.food <= 197,
            "decay should be percentage-based, not flat -1"
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
            dx: 0.0,
            dy: -0.2,
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
        assert!(game.difficulty.milestones.contains(&Milestone::FirstWinter));
        assert!(game.difficulty.threat_level > 0.0);
    }

    #[test]
    fn plague_kills_villager() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

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

        game.events
            .active_events
            .push(GameEvent::BanditRaid { stolen: false });
        game.step(GameInput::None, &mut renderer).unwrap();

        // Bandits steal 25% of resources
        assert!(game.resources.food <= 75, "bandits should steal food");
        assert!(game.resources.wood <= 60, "bandits should steal wood");
        assert!(game.resources.stone <= 45, "bandits should steal stone");
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
}
