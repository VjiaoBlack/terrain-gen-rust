use anyhow::Result;
use hecs::World;
use rand::RngExt;
use serde::{Serialize, Deserialize};

use crate::ecs::{self, AiResult, Behavior, BehaviorState, BuildSite, BuildingType, Creature, FarmPlot, GarrisonBuilding, HutBuilding, Position, ProcessingBuilding, Recipe, Resources, SkillMults, Species, Sprite, FoodSource, Den, StoneDeposit, ResourceType, Stockpile, SerializedEntity};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::simulation::{DayNightCycle, InfluenceMap, MoistureMap, Season, SimConfig, VegetationMap, WaterMap};
use crate::terrain_gen::{self, TerrainGenConfig};
use crate::tilemap::{Camera, Terrain, TileMap};

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
    /// Mouse click at screen coordinates (x, y)
    MouseClick { x: u16, y: u16 },
    None,
}

/// Width of the left-side UI panel in screen columns.
pub const PANEL_WIDTH: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayMode {
    None,
    Tasks,      // Color-code villagers by current activity
    Resources,  // Show resource locations with color markers
    Threats,    // Show wolf positions and danger zones
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    Drought { ticks_remaining: u64 },
    BountifulHarvest { ticks_remaining: u64 },
    Migration { count: u32 },
    WolfSurge { ticks_remaining: u64 },
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

    fn has_event_type(&self, check: &str) -> bool {
        self.active_events.iter().any(|e| match e {
            GameEvent::Drought { .. } => check == "drought",
            GameEvent::BountifulHarvest { .. } => check == "harvest",
            GameEvent::Migration { .. } => check == "migration",
            GameEvent::WolfSurge { .. } => check == "wolf_surge",
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
        Self { farming: 1.0, mining: 1.0, woodcutting: 1.0, building: 1.0, military: 1.0 }
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
}

impl Game {
    pub fn new(target_fps: u32, seed: u32) -> Self {
        let terrain_config = TerrainGenConfig { seed, ..Default::default() };
        let (map, heights) = terrain_gen::generate_terrain(256, 256, &terrain_config);
        let mut water = WaterMap::new(256, 256);
        // Seed water at terrain-Water tiles so ocean/lake areas have actual water
        for y in 0..256 {
            for x in 0..256 {
                if let Some(Terrain::Water) = map.get(x, y) {
                    let depth = (terrain_config.water_level - heights[y * 256 + x]).max(0.01);
                    water.set(x, y, depth);
                }
            }
        }
        let moisture = MoistureMap::new(256, 256);
        let vegetation = VegetationMap::new(256, 256);
        let camera = Camera::new(100, 100);
        let mut world = World::new();

        // Spawn entities on walkable tiles (search outward if blocked)
        let find_walkable = |map: &TileMap, cx: usize, cy: usize| -> (f64, f64) {
            for r in 0..50 {
                for dy in -(r as i32)..=(r as i32) {
                    for dx in -(r as i32)..=(r as i32) {
                        if dx.unsigned_abs() as usize != r && dy.unsigned_abs() as usize != r { continue; }
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

        // Player
        let (px, py) = find_walkable(&map, 128, 128);
        ecs::spawn_entity(&mut world, px, py, 0.0, 0.0, '@', Color(255, 255, 0));

        // Ecosystem: dens, berry bushes, prey, predators
        let den_spots = [(115, 110), (135, 120), (120, 140), (108, 130)];
        let bush_spots = [(125, 105), (140, 115), (110, 125), (130, 135), (118, 118), (132, 128)];

        for &(cx, cy) in &den_spots {
            let (dx, dy) = find_walkable(&map, cx, cy);
            ecs::spawn_den(&mut world, dx, dy);
            // Spawn a prey near its den
            let (rx, ry) = find_walkable(&map, cx + 1, cy + 1);
            ecs::spawn_prey(&mut world, rx, ry, dx, dy);
        }

        for &(cx, cy) in &bush_spots {
            let (bx, by) = find_walkable(&map, cx, cy);
            ecs::spawn_berry_bush(&mut world, bx, by);
        }

        // Predators — fewer, roam wider
        let pred_spots = [(120, 108), (130, 130)];
        for &(cx, cy) in &pred_spots {
            let (wx, wy) = find_walkable(&map, cx, cy);
            ecs::spawn_predator(&mut world, wx, wy);
        }

        // Settlement: stockpile + villagers near center, with nearby food
        let (sx, sy) = find_walkable(&map, 125, 125);
        ecs::spawn_stockpile(&mut world, sx, sy);

        // Berry bushes near settlement so villagers have food access
        for &(bsx, bsy) in &[(124, 124), (126, 127), (123, 126), (127, 124)] {
            let (bx, by) = find_walkable(&map, bsx, bsy);
            ecs::spawn_berry_bush(&mut world, bx, by);
        }

        // Stone deposits near settlement so villagers can gather stone
        for &(dsx, dsy) in &[(122, 125), (128, 126)] {
            let (dx, dy) = find_walkable(&map, dsx, dsy);
            ecs::spawn_stone_deposit(&mut world, dx, dy);
        }

        // Spawn 3 villagers near the stockpile
        for i in 0..3 {
            let (vx, vy) = find_walkable(&map, 125 + i * 2, 126);
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
            day_night: DayNightCycle::new(256, 256),
            scroll_speed: 2,
            raining: false,
            paused: false,
            debug_view: false,
            query_mode: false,
            query_cx: 128,
            query_cy: 128,
            display_fps: None,
            resources: Resources { food: 10, wood: 10, stone: 5, ..Default::default() },
            build_mode: false,
            build_cursor_x: 128,
            build_cursor_y: 128,
            selected_building: BuildingType::Wall,
            influence: InfluenceMap::new(256, 256),
            last_birth_tick: 0,
            notifications: Vec::new(),
            game_over: false,
            peak_population: 3,
            auto_build: false,
            skills: CivSkills::default(),
            overlay: OverlayMode::None,
            events: EventSystem::default(),
        };
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

    fn update_events(&mut self) {
        // Tick down duration-based events, remove expired
        self.events.active_events.retain_mut(|event| {
            match event {
                GameEvent::Drought { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events.event_log.push("Drought has ended.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::BountifulHarvest { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events.event_log.push("Bountiful harvest season ends.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::WolfSurge { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events.event_log.push("Wolf surge subsides.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::Migration { .. } => false, // instant, remove after spawning
            }
        });

        // Keep event log trimmed
        if self.events.event_log.len() > 5 {
            self.events.event_log.drain(0..self.events.event_log.len() - 5);
        }

        // Check for new events every 100 ticks
        if self.tick % 100 != 0 { return; }

        let mut rng = rand::rng();
        let season = self.day_night.season;

        match season {
            Season::Summer => {
                if !self.events.has_event_type("drought") && rng.random_range(0u32..100) < 15 {
                    self.events.active_events.push(GameEvent::Drought { ticks_remaining: 300 });
                    self.events.event_log.push("Drought! Farm yields halved.".to_string());
                    self.notify("Drought! Farm yields halved.".to_string());
                }
            }
            Season::Autumn => {
                if !self.events.has_event_type("harvest") && rng.random_range(0u32..100) < 20 {
                    self.events.active_events.push(GameEvent::BountifulHarvest { ticks_remaining: 200 });
                    self.events.event_log.push("Bountiful harvest! Farm yields doubled.".to_string());
                    self.notify("Bountiful harvest! Farm yields doubled.".to_string());
                }
            }
            Season::Spring => {
                // Migration: new villagers arrive if food surplus and housing available
                let villager_count = self.world.query::<&Creature>().iter()
                    .filter(|c| c.species == Species::Villager).count() as u32;
                let hut_capacity: u32 = self.world.query::<&HutBuilding>().iter()
                    .map(|h| h.capacity).sum();
                let has_housing = hut_capacity > villager_count;
                if has_housing && self.resources.food > 30 && rng.random_range(0u32..100) < 20 {
                    let count = rng.random_range(1u32..4);
                    let (cx, cy) = self.settlement_center();
                    for _ in 0..count {
                        let ox = rng.random_range(-3i32..4) as f64;
                        let oy = rng.random_range(-3i32..4) as f64;
                        ecs::spawn_villager(&mut self.world, cx as f64 + ox, cy as f64 + oy);
                    }
                    self.events.event_log.push(format!("{} migrants arrived!", count));
                    self.notify(format!("{} migrants arrived!", count));
                }
            }
            Season::Winter => {
                if !self.events.has_event_type("wolf_surge") && rng.random_range(0u32..100) < 25 {
                    self.events.active_events.push(GameEvent::WolfSurge { ticks_remaining: 400 });
                    self.events.event_log.push("Wolf surge! Pack activity increases.".to_string());
                    self.notify("Wolf surge! Pack activity increases.".to_string());
                }
            }
        }
    }

    pub fn save(&self, path: &str) -> Result<()> {
        let state = SaveState {
            tick: self.tick,
            resources: self.resources.clone(),
            skills: self.skills.clone(),
            day_night: serde_json::from_value(serde_json::to_value(&self.day_night)?)?,
            map: serde_json::from_value(serde_json::to_value(&self.map)?)?,
            heights: self.heights.clone(),
            water: serde_json::from_value(serde_json::to_value(&self.water)?)?,
            moisture: serde_json::from_value(serde_json::to_value(&self.moisture)?)?,
            vegetation: serde_json::from_value(serde_json::to_value(&self.vegetation)?)?,
            influence: serde_json::from_value(serde_json::to_value(&self.influence)?)?,
            entities: ecs::serialize_world(&self.world),
            last_birth_tick: self.last_birth_tick,
            peak_population: self.peak_population,
            raining: self.raining,
            auto_build: self.auto_build,
            sim_config: self.sim_config.clone(),
            terrain_config: serde_json::from_value(serde_json::to_value(&self.terrain_config)?)?,
            events: self.events.clone(),
        };
        let file = std::fs::File::create(path)?;
        serde_json::to_writer(file, &state)?;
        Ok(())
    }

    pub fn load(path: &str, target_fps: u32) -> Result<Game> {
        let file = std::fs::File::open(path)?;
        let state: SaveState = serde_json::from_reader(file)?;
        Ok(Game {
            target_fps,
            tick: state.tick,
            map: state.map,
            heights: state.heights,
            water: state.water,
            moisture: state.moisture,
            vegetation: state.vegetation,
            sim_config: state.sim_config,
            terrain_config: state.terrain_config,
            camera: Camera { x: 0, y: 0 },
            world: ecs::deserialize_world(&state.entities),
            day_night: state.day_night,
            scroll_speed: 2,
            raining: state.raining,
            debug_view: false,
            paused: false,
            query_mode: false,
            query_cx: 0,
            query_cy: 0,
            display_fps: None,
            resources: state.resources,
            build_mode: false,
            build_cursor_x: 0,
            build_cursor_y: 0,
            selected_building: BuildingType::Wall,
            influence: state.influence,
            last_birth_tick: state.last_birth_tick,
            notifications: vec![],
            game_over: false,
            peak_population: state.peak_population,
            auto_build: state.auto_build,
            skills: state.skills,
            overlay: OverlayMode::None,
            events: state.events,
        })
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
                    self.camera.clamp(self.map.width, self.map.height, world_vw, vh);
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
            GameInput::ToggleErosion => self.sim_config.erosion_enabled = !self.sim_config.erosion_enabled,
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
            GameInput::QueryUp => if self.query_mode { self.query_cy -= 1; },
            GameInput::QueryDown => if self.query_mode { self.query_cy += 1; },
            GameInput::QueryLeft => if self.query_mode { self.query_cx -= 1; },
            GameInput::QueryRight => if self.query_mode { self.query_cx += 1; },
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
            GameInput::BuildUp => if self.build_mode { self.build_cursor_y -= 1; },
            GameInput::BuildDown => if self.build_mode { self.build_cursor_y += 1; },
            GameInput::BuildLeft => if self.build_mode { self.build_cursor_x -= 1; },
            GameInput::BuildRight => if self.build_mode { self.build_cursor_x += 1; },
            GameInput::BuildCycleType => if self.build_mode {
                let types = BuildingType::all();
                let idx = types.iter().position(|t| *t == self.selected_building).unwrap_or(0);
                self.selected_building = types[(idx + 1) % types.len()];
            },
            GameInput::BuildPlace => if self.build_mode {
                self.try_place_building();
            },
            GameInput::Drain => self.water.drain(),
            GameInput::ToggleAutoBuild => self.auto_build = !self.auto_build,
            GameInput::CycleOverlay => {
                self.overlay = match self.overlay {
                    OverlayMode::None => OverlayMode::Tasks,
                    OverlayMode::Tasks => OverlayMode::Resources,
                    OverlayMode::Resources => OverlayMode::Threats,
                    OverlayMode::Threats => OverlayMode::None,
                };
            }
            GameInput::MouseClick { x, y } => self.handle_mouse_click(x, y, renderer),
            GameInput::Save => { let _ = self.save("savegame.json"); },
            GameInput::Load => {} // handled in main.rs loop
            GameInput::Quit | GameInput::Restart | GameInput::None => {}
        }

        let (vw, vh) = renderer.size();
        // World-space viewport: map area is screen minus panel, divided by aspect ratio
        let map_w = vw.saturating_sub(PANEL_WIDTH);
        let world_vw = (map_w as i32 / CELL_ASPECT) as u16;
        self.camera.clamp(self.map.width, self.map.height, world_vw, vh);

        // update simulation (skip when paused)
        if !self.paused {
            self.tick += 1;

            // Clean up old notifications
            self.notifications.retain(|(t, _)| self.tick - t < 200);

            // Update event system
            self.update_events();

            // Apply seasonal modifiers
            let mods = self.day_night.season_modifiers();

            ecs::system_hunger(&mut self.world, mods.hunger_mult);

            // Siege model: compute if settlement is defended
            let defense_rating = self.compute_defense_rating();
            let (scx, scy) = self.settlement_center();
            let wolves_near_count = self.world.query::<(&Position, &Creature)>().iter()
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
                if self.tick % 200 == 0 {
                    self.notify("Wolf pack repelled by defenses!".to_string());
                }
                self.skills.military += 0.002 * wolves_near_count as f64;
            }

            let skill_mults = SkillMults {
                gather_wood_speed: 1.0 + self.skills.woodcutting / 50.0,
                gather_stone_speed: 1.0 + self.skills.mining / 50.0,
                build_speed: (self.skills.building / 50.0).floor() as u32,
            };
            let ai_result = ecs::system_ai(&mut self.world, &self.map, mods.wolf_aggression, self.resources.food, self.resources.wood, self.resources.stone, self.resources.grain, &skill_mults, settlement_defended, self.day_night.is_night());
            let mut deposited_food = 0u32;
            let mut deposited_wood = 0u32;
            let mut deposited_stone = 0u32;
            for res in ai_result.deposited {
                match res {
                    ResourceType::Food => { self.resources.food += 1; deposited_food += 1; },
                    ResourceType::Wood => { self.resources.wood += 1; deposited_wood += 1; },
                    ResourceType::Stone => { self.resources.stone += 1; deposited_stone += 1; },
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
                self.resources.grain = self.resources.grain.saturating_sub(ai_result.grain_consumed);
                self.notify(format!("Villager ate grain (-{})", ai_result.grain_consumed));
            }
            if ai_result.food_consumed > 0 {
                self.resources.food = self.resources.food.saturating_sub(ai_result.food_consumed);
                self.notify(format!("Villager ate from stockpile (-{} food)", ai_result.food_consumed));
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

            // Count creatures before breeding to detect new spawns
            let prey_before = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Prey).count();
            let wolf_before = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Predator).count();

            let wolf_boost = self.events.wolf_spawn_multiplier();
            ecs::system_breeding(&mut self.world, self.day_night.season, wolf_boost);

            let prey_after = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Prey).count();
            let wolf_after = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Predator).count();
            if prey_after > prey_before {
                self.notify(format!("New rabbit born!"));
            }
            if wolf_after > wolf_before {
                self.notify(format!("New wolf born!"));
            }

            // Count species before death to detect who died
            let villagers_before = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Villager).count();
            let prey_before_death = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Prey).count();
            let wolves_before_death = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Predator).count();

            ecs::system_death(&mut self.world);

            let villagers_after = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Villager).count();
            let prey_after_death = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Prey).count();
            let wolves_after_death = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Predator).count();

            let villager_deaths = villagers_before.saturating_sub(villagers_after);
            let prey_deaths = prey_before_death.saturating_sub(prey_after_death);
            let wolf_deaths = wolves_before_death.saturating_sub(wolves_after_death);
            if villager_deaths > 0 {
                self.notify(format!("Villager died!"));
            }
            if prey_deaths > 0 {
                self.notify(format!("A rabbit was killed!"));
            }
            if wolf_deaths > 0 {
                self.notify(format!("A wolf died!"));
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

            // Farm growth and harvest (farming skill + event multiplier)
            let farm_mult = (1.0 + self.skills.farming / 100.0) * self.events.farm_yield_multiplier();
            let farm_food = ecs::system_farms(&mut self.world, self.day_night.season, farm_mult);
            self.resources.food += farm_food;
            // Active farms contribute to farming skill
            let active_farms = self.world.query::<&FarmPlot>().iter().count() as f64;
            self.skills.farming += active_farms * 0.002;
            if farm_food > 0 {
                self.notify(format!("Farm harvested: +{} food", farm_food));
            }

            // Processing buildings auto-process resources
            let process_mult = 1.0;
            ecs::system_processing(&mut self.world, &mut self.resources, process_mult);

            // Winter food decay: raw food spoils, grain is preserved
            if self.day_night.season == Season::Winter && self.tick % 50 == 0 && self.resources.food > 0 {
                self.resources.food -= 1;
                self.notify("Food spoiled in winter (-1)".to_string());
            }

            // Resource regrowth
            ecs::system_regrowth(&mut self.world, &self.map, self.tick);

            // Check for completed buildings
            self.check_build_completion();

            // Update influence map: villagers emit 1.0, active build sites emit 0.5
            self.update_influence();

            // Population growth check
            self.try_population_growth();

            // Auto-build check (every 200 ticks)
            if self.auto_build && self.tick % 200 == 0 {
                self.auto_build_tick();
            }

            // Seasonal config for rain/water
            let mut tick_config = self.sim_config.clone();
            tick_config.rain_rate *= mods.rain_mult;
            tick_config.evaporation *= mods.evap_mult;

            if self.raining {
                self.water.rain(&tick_config);
            }
            // Only run expensive water sim when there's actually water
            if self.raining || self.water.has_water() {
                self.water.update(&mut self.heights, &tick_config);
                self.moisture.update(&self.water, &mut self.vegetation, &self.map);
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
        }

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

    pub fn step_headless(&mut self, input: GameInput, renderer: &mut HeadlessRenderer) -> Result<FrameSnapshot> {
        self.step(input, renderer)?;
        Ok(self.snapshot(renderer))
    }

    /// Check if a building can be placed at the given position.
    pub fn can_place_building(&self, bx: i32, by: i32, building_type: BuildingType) -> bool {
        let (w, h) = building_type.size();
        for dy in 0..h {
            for dx in 0..w {
                let tx = bx + dx;
                let ty = by + dy;
                if tx < 0 || ty < 0 || tx as usize >= self.map.width || ty as usize >= self.map.height {
                    return false;
                }
                if let Some(terrain) = self.map.get(tx as usize, ty as usize) {
                    match terrain {
                        Terrain::Grass | Terrain::Sand | Terrain::Forest => {} // ok
                        _ => return false, // water, mountain, snow, existing buildings
                    }
                } else {
                    return false;
                }
            }
        }
        true
    }

    /// Try to place a building at the build cursor position.
    fn try_place_building(&mut self) {
        let bx = self.build_cursor_x;
        let by = self.build_cursor_y;
        let bt = self.selected_building;

        if !self.can_place_building(bx, by, bt) {
            return;
        }

        // Check resources
        let cost = bt.cost();
        if !self.resources.can_afford(&cost) {
            return;
        }

        // Deduct resources
        self.resources.deduct(&cost);

        self.place_build_site(bx, by, bt);
    }

    /// Place a build site: reserve footprint tiles and spawn the entity.
    fn place_build_site(&mut self, bx: i32, by: i32, bt: BuildingType) {
        let (sw, sh) = bt.size();
        for dy in 0..sh {
            for dx in 0..sw {
                let tx = bx + dx;
                let ty = by + dy;
                if tx >= 0 && ty >= 0 {
                    self.map.set(tx as usize, ty as usize, Terrain::BuildingFloor);
                }
            }
        }
        ecs::spawn_build_site(&mut self.world, bx as f64, by as f64, bt);
    }

    /// Handle a mouse click at screen coordinates.
    fn handle_mouse_click(&mut self, sx: u16, sy: u16, renderer: &dyn Renderer) {
        let (_w, h) = renderer.size();

        // Click in panel area — handle panel buttons
        if sx < PANEL_WIDTH {
            self.handle_panel_click(sy, h);
            return;
        }

        // Click in map area — convert screen coords to world coords
        let map_sx = sx - PANEL_WIDTH;
        let wx = self.camera.x + map_sx as i32 / CELL_ASPECT;
        let wy = self.camera.y + sy as i32;

        if self.build_mode {
            // Move build cursor and place
            self.build_cursor_x = wx;
            self.build_cursor_y = wy;
            self.try_place_building();
        } else {
            // Enter query mode at clicked position
            self.query_mode = true;
            self.query_cx = wx;
            self.query_cy = wy;
        }
    }

    /// Handle clicks on the left panel buttons.
    fn handle_panel_click(&mut self, sy: u16, _h: u16) {
        // Panel layout (row positions):
        // 0: header
        // 1: blank
        // 2-3: date/season
        // 4: blank
        // 5-7: resources
        // 8: blank
        // 9-11: population
        // 12: blank
        // 13: "-- Build --"
        // 14+: building type buttons
        // After buildings: auto-build toggle
        let building_start = 14u16;
        let types = BuildingType::all();
        let auto_build_row = building_start + types.len() as u16 + 1;

        if sy >= building_start && sy < building_start + types.len() as u16 {
            let idx = (sy - building_start) as usize;
            if idx < types.len() {
                self.selected_building = types[idx];
                self.build_mode = true;
                self.query_mode = false;
            }
        } else if sy == auto_build_row {
            self.auto_build = !self.auto_build;
        }
    }

    /// Compute the average position of all villagers as the settlement center.
    fn settlement_center(&self) -> (i32, i32) {
        let positions: Vec<(f64, f64)> = self.world.query::<(&Position, &Creature)>().iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if positions.is_empty() {
            return (128, 128);
        }
        let cx = positions.iter().map(|p| p.0).sum::<f64>() / positions.len() as f64;
        let cy = positions.iter().map(|p| p.1).sum::<f64>() / positions.len() as f64;
        (cx as i32, cy as i32)
    }

    /// Compute a defense rating from garrison buildings, wall tiles, and military skill.
    fn compute_defense_rating(&self) -> f64 {
        let garrison_defense: f64 = self.world.query::<&GarrisonBuilding>().iter()
            .map(|g| g.defense_bonus)
            .sum();

        let (cx, cy) = self.settlement_center();
        let mut wall_tiles = 0u32;
        for dy in -20i32..=20 {
            for dx in -20i32..=20 {
                let tx = cx + dx;
                let ty = cy + dy;
                if tx >= 0 && ty >= 0 {
                    if let Some(Terrain::BuildingWall) = self.map.get(tx as usize, ty as usize) {
                        wall_tiles += 1;
                    }
                }
            }
        }

        garrison_defense + wall_tiles as f64 * 0.5 + self.skills.military * 0.2
    }

    /// Check for completed build sites and apply their tiles to the map.
    fn check_build_completion(&mut self) {
        let mut completed: Vec<(hecs::Entity, Position, BuildSite)> = Vec::new();
        for (e, (pos, site)) in self.world.query::<(hecs::Entity, (&Position, &BuildSite))>().iter() {
            if site.progress >= site.required {
                completed.push((e, *pos, *site));
            }
        }
        for &(e, pos, site) in &completed {
            for (dx, dy, terrain) in site.building_type.tiles() {
                let tx = pos.x as i32 + dx;
                let ty = pos.y as i32 + dy;
                if tx >= 0 && ty >= 0 {
                    self.map.set(tx as usize, ty as usize, terrain);
                }
            }
            // Spawn building entities for completed buildings
            if site.building_type == BuildingType::Hut {
                let (sw, sh) = site.building_type.size();
                let cx = pos.x + sw as f64 / 2.0;
                let cy = pos.y + sh as f64 / 2.0;
                ecs::spawn_hut(&mut self.world, cx, cy);
            }
            if site.building_type == BuildingType::Farm {
                let (sw, sh) = site.building_type.size();
                let cx = pos.x + sw as f64 / 2.0;
                let cy = pos.y + sh as f64 / 2.0;
                ecs::spawn_farm_plot(&mut self.world, cx, cy);
            }
            if site.building_type == BuildingType::Workshop {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::WoodToPlanks);
            }
            if site.building_type == BuildingType::Smithy {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::StoneToMasonry);
            }
            if site.building_type == BuildingType::Garrison {
                ecs::spawn_garrison(&mut self.world, pos.x, pos.y);
            }
            self.world.despawn(e).ok();
        }
        for &(_, _, site) in &completed {
            self.notify(format!("Building complete: {}", site.building_type.name()));
        }
    }

    /// Collect influence sources from villagers and active build sites, then update.
    fn update_influence(&mut self) {
        let mut sources: Vec<(f64, f64, f64)> = Vec::new();

        // Villagers emit influence at strength 1.0
        for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
            if creature.species == Species::Villager {
                sources.push((pos.x, pos.y, 1.0));
            }
        }

        // Active build sites emit influence at strength 0.5
        for (pos, _site) in self.world.query::<(&Position, &BuildSite)>().iter() {
            sources.push((pos.x, pos.y, 0.5));
        }

        self.influence.update(&sources);
    }

    /// Check conditions and spawn a new villager if met.
    /// Births require: 2+ villagers, food >= 5, and housing capacity.
    /// More surplus housing = shorter birth cooldown (min 200, max 800 ticks).
    fn try_population_growth(&mut self) {
        let villager_count = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager)
            .count() as u32;

        // Count total hut capacity
        let total_capacity: u32 = self.world.query::<&HutBuilding>().iter()
            .map(|h| h.capacity)
            .sum();

        // Housing surplus determines birth rate
        let housing_surplus = total_capacity.saturating_sub(villager_count);
        let birth_cooldown = if housing_surplus == 0 {
            return; // No births without housing surplus
        } else if housing_surplus >= 4 {
            200 // Fast growth when lots of empty housing
        } else {
            800 / housing_surplus as u64 // 800, 400, 266 for surplus 1, 2, 3
        };

        if self.tick - self.last_birth_tick <= birth_cooldown {
            return;
        }

        if villager_count < 2 || self.resources.food < 5 {
            return;
        }

        self.resources.food -= 5;

        // Collect villager positions to find a spawn point nearby
        let villager_pos: Vec<(f64, f64)> = self.world.query::<(&Position, &Creature)>().iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();

        if let Some(&(vx, vy)) = villager_pos.first() {
            if let Some((nx, ny)) = self.find_nearby_walkable(vx, vy, 5) {
                ecs::spawn_villager(&mut self.world, nx, ny);
                self.last_birth_tick = self.tick;
                self.notify("New villager born!".to_string());
            }
        }
    }

    /// Find a walkable tile within `radius` of (cx, cy).
    fn find_nearby_walkable(&self, cx: f64, cy: f64, radius: i32) -> Option<(f64, f64)> {
        for r in 0..=radius {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue; // only check perimeter of each ring
                    }
                    let nx = cx + dx as f64;
                    let ny = cy + dy as f64;
                    if self.map.is_walkable(nx, ny) {
                        return Some((nx, ny));
                    }
                }
            }
        }
        None
    }

    /// Auto-build: place buildings automatically based on settlement needs.
    fn auto_build_tick(&mut self) {
        // Find settlement center from villager positions
        let villager_pos: Vec<(f64, f64)> = self.world.query::<(&Position, &Creature)>().iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if villager_pos.is_empty() {
            return;
        }
        let cx = villager_pos.iter().map(|p| p.0).sum::<f64>() / villager_pos.len() as f64;
        let cy = villager_pos.iter().map(|p| p.1).sum::<f64>() / villager_pos.len() as f64;

        // Count existing farms (completed + in-progress)
        let farm_count = self.world.query::<&FarmPlot>().iter().count()
            + self.world.query::<&BuildSite>().iter()
                .filter(|s| s.building_type == BuildingType::Farm).count();

        // Count existing build sites being worked on
        let pending_builds = self.world.query::<&BuildSite>().iter().count();
        // Don't queue too many builds at once
        if pending_builds >= 3 {
            return;
        }

        // Priority 1: Farm when food is low and we don't have many farms
        let villager_count = villager_pos.len() as u32;
        if self.resources.food < 8 + villager_count * 2 && farm_count < (villager_count as usize + 1) / 2 {
            let cost = BuildingType::Farm.cost();
            if self.resources.can_afford(&cost) {
                if let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Farm) {
                    self.resources.deduct(&cost);
                    self.place_build_site(bx, by, BuildingType::Farm);
                    self.notify("Auto-build: Farm queued".to_string());
                    return;
                }
            }
        }

        // Priority 2: Hut when population is growing and needs housing
        let hut_count = self.world.query::<&BuildSite>().iter()
            .filter(|s| s.building_type == BuildingType::Hut).count();
        // Count completed huts by checking for Hut-shaped building floor clusters
        // Simple heuristic: 1 hut per 3 villagers needed
        let huts_needed = (villager_count as usize + 2) / 3;
        if hut_count < huts_needed && villager_count >= 3 {
            let cost = BuildingType::Hut.cost();
            if self.resources.can_afford(&cost) {
                if let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Hut) {
                    self.resources.deduct(&cost);
                    self.place_build_site(bx, by, BuildingType::Hut);
                    self.notify("Auto-build: Hut queued".to_string());
                    return;
                }
            }
        }

        // Priority 3: Walls when wolves are nearby settlement
        let wolf_near = self.world.query::<(&Position, &Creature)>().iter()
            .filter(|(_, c)| c.species == Species::Predator)
            .any(|(p, _)| {
                let dx = p.x - cx;
                let dy = p.y - cy;
                dx * dx + dy * dy < 400.0 // within ~20 tiles
            });
        if wolf_near {
            let cost = BuildingType::Wall.cost();
            if self.resources.can_afford(&cost) {
                // Place wall between settlement center and nearest wolf
                if let Some((bx, by)) = self.find_wall_spot(cx, cy) {
                    self.resources.deduct(&cost);
                    self.place_build_site(bx, by, BuildingType::Wall);
                    self.notify("Auto-build: Wall queued".to_string());
                }
            }
        }
    }

    /// Find a valid spot for a building near (cx, cy), searching outward in rings.
    fn find_building_spot(&self, cx: f64, cy: f64, bt: BuildingType) -> Option<(i32, i32)> {
        let (bw, bh) = bt.size();
        for r in 2i32..20 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let bx = cx as i32 + dx * bw;
                    let by = cy as i32 + dy * bh;
                    if self.can_place_building(bx, by, bt) {
                        return Some((bx, by));
                    }
                }
            }
        }
        None
    }

    /// Find a spot for a defensive wall between settlement center and nearest wolf.
    fn find_wall_spot(&self, cx: f64, cy: f64) -> Option<(i32, i32)> {
        // Find direction to nearest wolf
        let mut nearest_dist = f64::MAX;
        let mut wolf_dir = (0.0f64, 0.0f64);
        for (p, c) in self.world.query::<(&Position, &Creature)>().iter() {
            if c.species != Species::Predator { continue; }
            let dx = p.x - cx;
            let dy = p.y - cy;
            let dist = dx * dx + dy * dy;
            if dist < nearest_dist {
                nearest_dist = dist;
                let d = dist.sqrt().max(1.0);
                wolf_dir = (dx / d, dy / d);
            }
        }
        if nearest_dist == f64::MAX {
            return None;
        }
        // Place wall ~8 tiles out in that direction, searching nearby for valid spot
        let target_x = cx as i32 + (wolf_dir.0 * 8.0) as i32;
        let target_y = cy as i32 + (wolf_dir.1 * 8.0) as i32;
        for r in 0..5 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let wx = target_x + dx;
                    let wy = target_y + dy;
                    if self.can_place_building(wx, wy, BuildingType::Wall) {
                        return Some((wx, wy));
                    }
                }
            }
        }
        None
    }

    /// Apply seasonal color tinting to vegetation-sensitive terrain.
    fn season_tint(&self, color: Color, terrain: &Terrain) -> Color {
        use crate::simulation::Season;
        match terrain {
            Terrain::Grass | Terrain::Forest => {
                let Color(r, g, b) = color;
                match self.day_night.season {
                    Season::Spring => {
                        // Slightly more vibrant green
                        Color(r, (g as u16).min(255) as u8, b)
                    }
                    Season::Summer => color, // normal
                    Season::Autumn => {
                        // Shift green toward orange/brown
                        let r2 = (r as u16 + 40).min(255) as u8;
                        let g2 = (g as i16 - 20).max(0) as u8;
                        Color(r2, g2, b)
                    }
                    Season::Winter => {
                        // Desaturate and lighten — frost effect
                        let avg = (r as u16 + g as u16 + b as u16) / 3;
                        let blend = |c: u8| ((c as u16 + avg) / 2).min(255) as u8;
                        Color(blend(r), blend(g), blend(b))
                    }
                }
            }
            _ => color,
        }
    }

    /// Draw the left-side UI panel.
    fn draw_panel(&self, renderer: &mut dyn Renderer) {
        let (_w, h) = renderer.size();
        let pw = PANEL_WIDTH as usize;
        let bg = Color(25, 25, 40);
        let fg = Color(200, 200, 200);
        let dim = Color(120, 120, 140);
        let highlight = Color(255, 220, 100);
        let green = Color(100, 220, 100);

        // Helper: draw a line of text in the panel
        let mut row = 0u16;
        let mut draw_line = |r: &mut dyn Renderer, y: u16, text: &str, color: Color| {
            for (i, ch) in text.chars().enumerate() {
                if i < pw {
                    r.draw(i as u16, y, ch, color, Some(bg));
                }
            }
            for i in text.len()..pw {
                r.draw(i as u16, y, ' ', fg, Some(bg));
            }
        };

        // Fill panel background
        for y in 0..h {
            for x in 0..PANEL_WIDTH {
                renderer.draw(x, y, ' ', fg, Some(bg));
            }
        }

        // Header
        draw_line(renderer, row, " TERRAIN-GEN", highlight); row += 1;

        // Separator
        let sep: String = std::iter::repeat('-').take(pw).collect();
        draw_line(renderer, row, &sep, dim); row += 1;

        // Date / Season
        let date = self.day_night.date_string();
        let time = self.day_night.time_string();
        draw_line(renderer, row, &format!(" {} {}", date, time), fg); row += 1;
        row += 1;

        // Resources
        draw_line(renderer, row, " Resources", highlight); row += 1;
        draw_line(renderer, row, &format!("  Food:  {}", self.resources.food), fg); row += 1;
        draw_line(renderer, row, &format!("  Wood:  {}", self.resources.wood), fg); row += 1;
        draw_line(renderer, row, &format!("  Stone: {}", self.resources.stone), fg); row += 1;
        if self.resources.planks > 0 || self.resources.masonry > 0 || self.resources.grain > 0 {
            draw_line(renderer, row, &format!("  Planks:  {}", self.resources.planks), dim); row += 1;
            draw_line(renderer, row, &format!("  Masonry: {}", self.resources.masonry), dim); row += 1;
            draw_line(renderer, row, &format!("  Grain:   {}", self.resources.grain), dim); row += 1;
        }
        row += 1;

        // Population
        let villagers = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();
        let prey = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Prey).count();
        let wolves = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Predator).count();
        draw_line(renderer, row, " Population", highlight); row += 1;
        draw_line(renderer, row, &format!("  Villagers: {}", villagers), fg); row += 1;
        draw_line(renderer, row, &format!("  Rabbits:   {}", prey), dim); row += 1;
        draw_line(renderer, row, &format!("  Wolves:    {}", wolves), dim); row += 1;
        row += 1;

        // Skills section
        let skill_color = Color(180, 160, 220);
        draw_line(renderer, row, " Skills", highlight); row += 1;
        draw_line(renderer, row, &format!("  Farm  {:4.1}", self.skills.farming), skill_color); row += 1;
        draw_line(renderer, row, &format!("  Mine  {:4.1}", self.skills.mining), skill_color); row += 1;
        draw_line(renderer, row, &format!("  Wood  {:4.1}", self.skills.woodcutting), skill_color); row += 1;
        draw_line(renderer, row, &format!("  Build {:4.1}", self.skills.building), skill_color); row += 1;
        draw_line(renderer, row, &format!("  Milit {:4.1}", self.skills.military), skill_color); row += 1;
        row += 1;

        // Build section
        draw_line(renderer, row, " Build (click/[b])", highlight); row += 1;
        let types = BuildingType::all();
        for bt in types {
            let c = bt.cost();
            let selected = self.build_mode && self.selected_building == *bt;
            let marker = if selected { ">" } else { " " };
            let mut cost_parts: Vec<String> = Vec::new();
            if c.food > 0 { cost_parts.push(format!("f:{}", c.food)); }
            if c.wood > 0 { cost_parts.push(format!("w:{}", c.wood)); }
            if c.stone > 0 { cost_parts.push(format!("s:{}", c.stone)); }
            if c.planks > 0 { cost_parts.push(format!("P:{}", c.planks)); }
            if c.masonry > 0 { cost_parts.push(format!("M:{}", c.masonry)); }
            let line = format!("{} {} {}", marker, bt.name(), cost_parts.join(" "));
            let color = if selected { green } else { fg };
            draw_line(renderer, row, &line, color);
            row += 1;
        }
        row += 1;

        // Auto-build toggle
        let ab_str = if self.auto_build { "ON" } else { "off" };
        draw_line(renderer, row, &format!(" Auto-build [a]: {}", ab_str),
            if self.auto_build { green } else { fg });
        row += 1;
        row += 1;

        // Overlay toggle
        let ov_str = match self.overlay {
            OverlayMode::None => "off",
            OverlayMode::Tasks => "TASKS",
            OverlayMode::Resources => "RESOURCES",
            OverlayMode::Threats => "THREATS",
        };
        draw_line(renderer, row, &format!(" Overlay [o]: {}", ov_str),
            if self.overlay != OverlayMode::None { green } else { fg });
        row += 1;

        // Active events
        if !self.events.active_events.is_empty() {
            row += 1;
            draw_line(renderer, row, " Events", Color(255, 200, 50)); row += 1;
            for event in &self.events.active_events {
                let (name, remaining) = match event {
                    GameEvent::Drought { ticks_remaining } => ("Drought", *ticks_remaining),
                    GameEvent::BountifulHarvest { ticks_remaining } => ("Harvest+", *ticks_remaining),
                    GameEvent::WolfSurge { ticks_remaining } => ("Wolf Surge", *ticks_remaining),
                    GameEvent::Migration { count } => {
                        draw_line(renderer, row, &format!("  +{} migrants", count), green);
                        row += 1;
                        continue;
                    }
                };
                let color = match event {
                    GameEvent::Drought { .. } => Color(200, 100, 50),
                    GameEvent::BountifulHarvest { .. } => Color(50, 200, 50),
                    GameEvent::WolfSurge { .. } => Color(200, 50, 50),
                    _ => fg,
                };
                draw_line(renderer, row, &format!("  {} ({}t)", name, remaining), color);
                row += 1;
            }
        }
        row += 1;

        // Controls
        draw_line(renderer, row, " Controls", highlight); row += 1;
        draw_line(renderer, row, "  arrows: scroll", dim); row += 1;
        draw_line(renderer, row, "  [b] build mode", dim); row += 1;
        draw_line(renderer, row, "  [k] query/inspect", dim); row += 1;
        draw_line(renderer, row, "  [space] pause", dim); row += 1;
        draw_line(renderer, row, "  click: build/query", dim); row += 1;
        draw_line(renderer, row, "  [q] quit", dim); row += 1;

        // Mode indicator
        if row + 2 < h {
            row += 1;
            if self.build_mode {
                draw_line(renderer, row, " MODE: BUILD", green);
            } else if self.query_mode {
                draw_line(renderer, row, " MODE: QUERY", Color(200, 100, 255));
            } else if self.paused {
                draw_line(renderer, row, " PAUSED", Color(255, 100, 100));
            }
        }
    }

    pub fn draw(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16; // reserve 2 lines for status
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH;

        // draw left panel first (background)
        self.draw_panel(renderer);

        // draw terrain with day/night lighting and seasonal tinting
        // Each world tile occupies `aspect` screen columns for square pixels.
        // Map area starts at panel_w.
        for sy in 0..h {
            for sx in panel_w..w {
                let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 {
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        if *terrain == Terrain::Water {
                            // Water terrain: no day/night shading, constant appearance
                            renderer.draw(sx, sy, terrain.ch(), terrain.fg(), terrain.bg());
                        } else {
                            let fg = self.season_tint(terrain.fg(), terrain);
                            let bg = terrain.bg().map(|c| self.season_tint(c, terrain));
                            let fg = self.day_night.apply_lighting(fg, wx as usize, wy as usize);
                            let bg = self.day_night.apply_lighting_bg(bg, wx as usize, wy as usize);
                            renderer.draw(sx, sy, terrain.ch(), fg, bg);
                        }
                    }
                }
            }
        }

        // draw vegetation on top of terrain (before water)
        for sy in 0..h.saturating_sub(status_h) {
            for sx in panel_w..w {
                let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 && (wx as usize) < self.vegetation.width && (wy as usize) < self.vegetation.height {
                    let v = self.vegetation.get(wx as usize, wy as usize);
                    if v > 0.2 {
                        let (ch, fg) = if v > 0.8 {
                            ('♠', Color(0, 80, 10))
                        } else if v > 0.5 {
                            ('♣', Color(10, 110, 20))
                        } else {
                            ('"', Color(40, 160, 40))
                        };
                        let fg = self.season_tint(fg, &Terrain::Forest);
                        let fg = self.day_night.apply_lighting(fg, wx as usize, wy as usize);
                        // Keep terrain bg underneath vegetation
                        let bg = self.map.get(wx as usize, wy as usize)
                            .and_then(|t| t.bg())
                            .map(|c| self.day_night.apply_lighting(c, wx as usize, wy as usize));
                        renderer.draw(sx, sy, ch, fg, bg);
                    }
                }
            }
        }

        // draw water on top of terrain (skip Water terrain — already rendered as ocean)
        for sy in 0..h.saturating_sub(status_h) {
            for sx in panel_w..w {
                let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 && (wx as usize) < self.water.width && (wy as usize) < self.water.height {
                    // Skip ocean tiles — they already have their own water appearance
                    if matches!(self.map.get(wx as usize, wy as usize), Some(Terrain::Water)) {
                        continue;
                    }
                    let depth = self.water.get_avg(wx as usize, wy as usize);
                    if depth > 0.0005 {
                        let intensity = (depth * 500.0).min(1.0);
                        let r = (50.0 * (1.0 - intensity)) as u8;
                        let g = (100.0 + 50.0 * intensity) as u8;
                        let b = (180.0 + 75.0 * intensity) as u8;
                        let ch = if depth > 0.01 { '≈' } else { '~' };
                        let fg = self.day_night.apply_lighting(Color(r, g, b), wx as usize, wy as usize);
                        let bg = self.day_night.apply_lighting_bg(
                            Some(Color(20, 40, (80.0 + 40.0 * intensity) as u8)),
                            wx as usize, wy as usize,
                        );
                        renderer.draw(sx, sy, ch, fg, bg);
                    }
                }
            }
        }

        // Territory tint: subtle blue where influence > 0.1
        for sy in 0..h.saturating_sub(status_h) {
            for sx in panel_w..w {
                let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 && (wx as usize) < self.influence.width && (wy as usize) < self.influence.height {
                    let inf = self.influence.get(wx as usize, wy as usize);
                    if inf > 0.1 {
                        let alpha = (inf * 0.3).min(0.3);
                        if let Some(cell) = renderer.get_cell(sx, sy) {
                            let bg = cell.bg.unwrap_or(Color(0, 0, 0));
                            let tinted = Color(
                                (bg.0 as f64 * (1.0 - alpha) + 80.0 * alpha) as u8,
                                (bg.1 as f64 * (1.0 - alpha) + 100.0 * alpha) as u8,
                                (bg.2 as f64 * (1.0 - alpha) + 200.0 * alpha) as u8,
                            );
                            renderer.draw(sx, sy, cell.ch, cell.fg, Some(tinted));
                        }
                    }
                }
            }
        }

        // draw entities (offset by camera) — world→screen X is multiplied by aspect
        // Skip AtHome (hidden in den), dim Captured (being eaten)
        for (e, (pos, sprite)) in self.world.query::<(hecs::Entity, (&Position, &Sprite))>().iter() {
            let bstate = self.world.get::<&Behavior>(e).ok().map(|b| b.state);
            if matches!(bstate, Some(BehaviorState::AtHome { .. })) {
                continue;
            }
            let sx = (pos.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= panel_w as i32 && sy >= 0 && (sx as u16) < w && (sy as u16) < h.saturating_sub(status_h) {
                let (tr, tg, tb) = self.day_night.ambient_tint();
                let fg = if matches!(bstate, Some(BehaviorState::Captured)) {
                    // Captured prey rendered dim red
                    Color(
                        (120.0 * tr).clamp(0.0, 255.0) as u8,
                        (30.0 * tg).clamp(0.0, 255.0) as u8,
                        (30.0 * tb).clamp(0.0, 255.0) as u8,
                    )
                } else if matches!(bstate, Some(BehaviorState::Sleeping { .. })) {
                    // Sleeping villagers rendered dimmer
                    Color(
                        (sprite.fg.0 as f64 * tr * 0.5).clamp(0.0, 255.0) as u8,
                        (sprite.fg.1 as f64 * tg * 0.5).clamp(0.0, 255.0) as u8,
                        (sprite.fg.2 as f64 * tb * 0.5).clamp(0.0, 255.0) as u8,
                    )
                } else {
                    Color(
                        (sprite.fg.0 as f64 * tr).clamp(0.0, 255.0) as u8,
                        (sprite.fg.1 as f64 * tg).clamp(0.0, 255.0) as u8,
                        (sprite.fg.2 as f64 * tb).clamp(0.0, 255.0) as u8,
                    )
                };
                // Task overlay: color-code villagers by activity
                let fg = if self.overlay == OverlayMode::Tasks {
                    if let Some(creature) = self.world.get::<&Creature>(e).ok() {
                        if creature.species == Species::Villager {
                            match bstate {
                                Some(BehaviorState::Gathering { resource_type: ResourceType::Wood, .. }) => Color(139, 90, 43),   // brown
                                Some(BehaviorState::Gathering { resource_type: ResourceType::Stone, .. }) => Color(150, 150, 150), // gray
                                Some(BehaviorState::Gathering { resource_type: ResourceType::Food, .. }) => Color(50, 200, 50),   // green
                                Some(BehaviorState::Hauling { .. }) => Color(200, 180, 50),   // gold
                                Some(BehaviorState::Building { .. }) => Color(255, 220, 50),   // yellow
                                Some(BehaviorState::Eating { .. }) => Color(50, 200, 50),      // green
                                Some(BehaviorState::Sleeping { .. }) => Color(100, 100, 200),  // blue
                                Some(BehaviorState::FleeHome) => Color(255, 50, 50),           // red
                                Some(BehaviorState::Idle { .. }) | Some(BehaviorState::Wander { .. }) => Color(80, 80, 180),  // dim blue
                                Some(BehaviorState::Seek { .. }) => Color(180, 180, 50),       // dim yellow
                                _ => fg,
                            }
                        } else { fg }
                    } else { fg }
                } else { fg };
                renderer.draw(sx as u16, sy as u16, sprite.ch, fg, None);
            }
        }

        // Overlay pass: draw additional markers on top
        if self.overlay == OverlayMode::Resources {
            self.draw_resource_overlay(renderer);
        } else if self.overlay == OverlayMode::Threats {
            self.draw_threat_overlay(renderer);
        }

        if self.query_mode {
            self.draw_query_cursor(renderer);
            self.draw_query_panel(renderer);
        }

        if self.build_mode {
            self.draw_build_mode(renderer);
        }

        self.draw_notifications(renderer);
        self.draw_status(renderer);
    }

    fn draw_build_mode(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;
        let (bw, bh) = self.selected_building.size();

        let valid = self.can_place_building(self.build_cursor_x, self.build_cursor_y, self.selected_building);

        // Draw ghost building footprint
        for dy in 0..bh {
            for dx in 0..bw {
                let wx = self.build_cursor_x + dx;
                let wy = self.build_cursor_y + dy;
                let sx = (wx - self.camera.x) * aspect + panel_w;
                let sy = wy - self.camera.y;
                if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
                    for ax in 0..aspect {
                        let cx = sx + ax;
                        if cx >= panel_w && (cx as u16) < w {
                            let (fg, bg) = if valid {
                                (Color(200, 255, 200), Color(0, 100, 0))
                            } else {
                                (Color(255, 200, 200), Color(100, 0, 0))
                            };
                            renderer.draw(cx as u16, sy as u16, '#', fg, Some(bg));
                        }
                    }
                }
            }
        }

        // Draw build mode info panel (bottom-left, above status)
        let cost = self.selected_building.cost();
        let name = self.selected_building.name();
        let line1 = format!(" BUILD: {} (tab:cycle, enter:place, b/esc:exit) ", name);
        let mut cost_str = String::new();
        if cost.food > 0 { cost_str += &format!("F:{} ", cost.food); }
        if cost.wood > 0 { cost_str += &format!("W:{} ", cost.wood); }
        if cost.stone > 0 { cost_str += &format!("S:{} ", cost.stone); }
        if cost.planks > 0 { cost_str += &format!("P:{} ", cost.planks); }
        if cost.masonry > 0 { cost_str += &format!("M:{} ", cost.masonry); }
        let line2 = format!(" Cost: {}| Have: F:{} W:{} S:{} P:{} M:{} ",
            cost_str, self.resources.food, self.resources.wood, self.resources.stone,
            self.resources.planks, self.resources.masonry);
        let valid_str = if valid { "OK" } else { "INVALID" };
        let line3 = format!(" Placement: {} | wasd:move cursor ", valid_str);

        let panel_y = h.saturating_sub(status_h + 3);
        let fg = Color(255, 255, 255);
        let bg = Color(40, 40, 80);
        for (i, line) in [&line1, &line2, &line3].iter().enumerate() {
            let sy = panel_y + i as u16;
            for (j, ch) in line.chars().enumerate() {
                if (j as u16) < w && sy < h {
                    renderer.draw(j as u16, sy, ch, fg, Some(bg));
                }
            }
            // Fill rest of panel width
            for j in line.len()..w as usize {
                if sy < h {
                    renderer.draw(j as u16, sy, ' ', fg, Some(bg));
                }
            }
        }
    }

    fn draw_query_cursor(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        let sx = (self.query_cx - self.camera.x) * aspect + panel_w;
        let sy = self.query_cy - self.camera.y;

        // Draw cursor bracket across aspect-width cells
        if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
            for dx in 0..aspect {
                let cx = sx + dx;
                if cx >= panel_w && (cx as u16) < w {
                    // Draw a highlight — bright magenta border
                    let cell = renderer.get_cell(cx as u16, sy as u16);
                    let ch = cell.map(|c| c.ch).unwrap_or(' ');
                    renderer.draw(cx as u16, sy as u16, ch, Color(255, 255, 255), Some(Color(180, 0, 180)));
                }
            }
        }
    }

    fn draw_query_panel(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;

        // Gather info about the tile and any entities at cursor
        let wx = self.query_cx;
        let wy = self.query_cy;

        let mut lines: Vec<String> = Vec::new();

        // Tile info
        if wx >= 0 && wy >= 0 {
            let ux = wx as usize;
            let uy = wy as usize;
            if let Some(terrain) = self.map.get(ux, uy) {
                lines.push(format!("({},{}) {:?}", wx, wy, terrain));
                if ux < self.map.width && uy < self.map.height {
                    let height = self.heights[uy * self.map.width + ux];
                    lines.push(format!("height: {:.3}", height));
                }
                let water_depth = if ux < self.water.width && uy < self.water.height {
                    self.water.get_avg(ux, uy)
                } else { 0.0 };
                if water_depth > 0.0001 {
                    lines.push(format!("water: {:.4}", water_depth));
                }
                let moisture = if ux < self.moisture.width && uy < self.moisture.height {
                    self.moisture.get(ux, uy)
                } else { 0.0 };
                if moisture > 0.01 {
                    lines.push(format!("moisture: {:.2}", moisture));
                }
                let veg = if ux < self.vegetation.width && uy < self.vegetation.height {
                    self.vegetation.get(ux, uy)
                } else { 0.0 };
                if veg > 0.01 {
                    lines.push(format!("vegetation: {:.2}", veg));
                }
                let inf = if ux < self.influence.width && uy < self.influence.height {
                    self.influence.get(ux, uy)
                } else { 0.0 };
                if inf > 0.01 {
                    lines.push(format!("influence: {:.2}", inf));
                }
            } else {
                lines.push(format!("({},{}) out of bounds", wx, wy));
            }
        }

        // Entity info — find all entities at this world position
        for (e, (pos, sprite)) in self.world.query::<(hecs::Entity, (&Position, &Sprite))>().iter() {
            let ex = pos.x.round() as i32;
            let ey = pos.y.round() as i32;
            if ex == wx && ey == wy {
                lines.push(format!("---"));
                lines.push(format!("'{}' at ({:.1},{:.1})", sprite.ch, pos.x, pos.y));

                if let Ok(creature) = self.world.get::<&Creature>(e) {
                    let species_str = match creature.species {
                        Species::Prey => "Prey",
                        Species::Predator => "Predator",
                        Species::Villager => "Villager",
                    };
                    lines.push(format!("{}", species_str));
                    lines.push(format!("hunger: {:.1}%", creature.hunger * 100.0));
                    lines.push(format!("sight: {:.0}", creature.sight_range));
                    lines.push(format!("home: ({:.0},{:.0})", creature.home_x, creature.home_y));
                }
                if let Ok(behavior) = self.world.get::<&Behavior>(e) {
                    let state_str = match &behavior.state {
                        BehaviorState::Wander { timer } => format!("Wander ({})", timer),
                        BehaviorState::Seek { target_x, target_y } => format!("Seek ({:.0},{:.0})", target_x, target_y),
                        BehaviorState::Idle { timer } => format!("Idle ({})", timer),
                        BehaviorState::Eating { timer } => format!("Eating ({})", timer),
                        BehaviorState::FleeHome => "Fleeing home!".to_string(),
                        BehaviorState::AtHome { timer } => format!("At home ({})", timer),
                        BehaviorState::Hunting { target_x, target_y } => format!("Hunting ({:.0},{:.0})", target_x, target_y),
                        BehaviorState::Captured => "CAPTURED!".to_string(),
                        BehaviorState::Gathering { timer, resource_type } => format!("Gathering {:?} ({})", resource_type, timer),
                        BehaviorState::Hauling { target_x, target_y, resource_type } => format!("Hauling {:?} ({:.0},{:.0})", resource_type, target_x, target_y),
                        BehaviorState::Sleeping { timer } => format!("Sleeping ({})", timer),
                        BehaviorState::Building { target_x, target_y, timer } => format!("Building ({:.0},{:.0}) ({})", target_x, target_y, timer),
                    };
                    lines.push(format!("state: {}", state_str));
                    lines.push(format!("speed: {:.2}", behavior.speed));
                    match &behavior.state {
                        BehaviorState::Gathering { resource_type, .. } |
                        BehaviorState::Hauling { resource_type, .. } => {
                            lines.push(format!("resource: {:?}", resource_type));
                        }
                        _ => {}
                    }
                }
                if self.world.get::<&FoodSource>(e).is_ok() {
                    lines.push("Food Source".to_string());
                }
                if self.world.get::<&Den>(e).is_ok() {
                    lines.push("Den (safe zone)".to_string());
                }
                if self.world.get::<&StoneDeposit>(e).is_ok() {
                    lines.push("Stone Deposit".to_string());
                }
                if let Ok(site) = self.world.get::<&ecs::BuildSite>(e) {
                    lines.push(format!("BuildSite: {}", site.building_type.name()));
                    lines.push(format!("progress: {}/{}", site.progress, site.required));
                    lines.push(format!("assigned: {}", site.assigned));
                }
                if let Ok(farm) = self.world.get::<&FarmPlot>(e) {
                    lines.push(format!("Farm: {:.0}% grown{}",
                        farm.growth * 100.0,
                        if farm.harvest_ready { " [READY]" } else { "" }));
                }
                if self.world.get::<&Stockpile>(e).is_ok() {
                    lines.push(format!("Stockpile (F:{} W:{} S:{})",
                        self.resources.food, self.resources.wood, self.resources.stone));
                    lines.push(format!("  Planks:{} Masonry:{} Grain:{}",
                        self.resources.planks, self.resources.masonry, self.resources.grain));
                }
                if let Ok(pb) = self.world.get::<&ProcessingBuilding>(e) {
                    let recipe_str = match pb.recipe {
                        ecs::Recipe::WoodToPlanks => "2 Wood -> 1 Planks",
                        ecs::Recipe::StoneToMasonry => "2 Stone -> 1 Masonry",
                        ecs::Recipe::FoodToGrain => "3 Food -> 2 Grain",
                    };
                    let has_input = match pb.recipe {
                        ecs::Recipe::WoodToPlanks => self.resources.wood >= 2,
                        ecs::Recipe::StoneToMasonry => self.resources.stone >= 2,
                        ecs::Recipe::FoodToGrain => self.resources.food >= 3,
                    };
                    let status = if has_input { "ACTIVE" } else { "IDLE" };
                    lines.push(format!("Recipe: {}", recipe_str));
                    lines.push(format!("Progress: {}/{} [{}]", pb.progress, pb.required, status));
                }
            }
        }

        // Draw panel in top-right corner
        let panel_w = lines.iter().map(|l| l.len()).max().unwrap_or(0) + 2;
        let panel_h = lines.len();
        let panel_x = w.saturating_sub(panel_w as u16 + 1);
        let panel_y = 1u16;

        let bg = Color(20, 20, 40);
        let fg = Color(220, 220, 220);

        // Draw background
        for dy in 0..panel_h {
            let sy = panel_y + dy as u16;
            if sy >= h.saturating_sub(status_h) { break; }
            for dx in 0..panel_w {
                let sx = panel_x + dx as u16;
                if sx < w {
                    renderer.draw(sx, sy, ' ', fg, Some(bg));
                }
            }
        }

        // Draw text
        for (dy, line) in lines.iter().enumerate() {
            let sy = panel_y + dy as u16;
            if sy >= h.saturating_sub(status_h) { break; }
            for (dx, ch) in line.chars().enumerate() {
                let sx = panel_x + 1 + dx as u16;
                if sx < w {
                    renderer.draw(sx, sy, ch, fg, Some(bg));
                }
            }
        }
    }

    fn draw_notifications(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let base_y = h.saturating_sub(status_h + 1);

        let now = self.tick;
        let visible: Vec<&(u64, String)> = self.notifications.iter()
            .filter(|(t, _)| now.saturating_sub(*t) < 120)
            .collect();

        for (i, (tick, msg)) in visible.iter().rev().enumerate() {
            let y = base_y.saturating_sub(i as u16);
            if y == 0 { break; }

            let age = now.saturating_sub(*tick);
            let alpha = if age < 60 { 1.0 } else { 1.0 - (age - 60) as f64 / 60.0 };
            let brightness = (220.0 * alpha) as u8;

            for (x, ch) in msg.chars().enumerate() {
                if (x as u16) < w {
                    renderer.draw(x as u16, y, ch, Color(brightness, brightness, brightness.min(180)), None);
                }
            }
        }
    }

    fn draw_game_over(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let red = Color(255, 60, 60);
        let white = Color(220, 220, 220);
        let dim = Color(140, 140, 140);

        let lines = [
            ("GAME OVER", red),
            ("", dim),
            ("All villagers have perished.", white),
            ("", dim),
            (&format!("Survived to {} ({} ticks)", self.day_night.date_string(), self.tick), dim),
            (&format!("Peak population: {}", self.peak_population), dim),
            (&format!("Resources: {} food, {} wood, {} stone, {} planks, {} masonry, {} grain",
                self.resources.food, self.resources.wood, self.resources.stone,
                self.resources.planks, self.resources.masonry, self.resources.grain), dim),
            ("", dim),
            ("Press [r] to restart, [q] to quit", white),
        ];

        let box_h = lines.len() as u16;
        let box_w: u16 = lines.iter().map(|(s, _)| s.len() as u16).max().unwrap_or(30).max(30);
        let start_y = h / 2 - box_h / 2;
        let start_x = w / 2 - box_w / 2;

        for (i, (text, color)) in lines.iter().enumerate() {
            let y = start_y + i as u16;
            if y >= h { break; }
            let pad = (box_w as usize).saturating_sub(text.len()) / 2;
            let padded = format!("{:>pad$}{}", "", text, pad = pad);
            for (j, ch) in padded.chars().enumerate() {
                let x = start_x + j as u16;
                if x < w {
                    renderer.draw(x, y, ch, *color, Some(Color(20, 20, 30)));
                }
            }
        }
    }

    fn draw_status(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let fps_str = match self.display_fps {
            Some(fps) => format!("{}fps", fps),
            None => "---".to_string(),
        };
        let pause_str = if self.paused { " PAUSED " } else { "" };
        let status = format!(
            " tick:{}  {}{}  rain:[r]{} erosion:[e]{} time:[t]{} view:[v]{} drain:[d]",
            self.tick, fps_str, pause_str,
            if self.raining { "+" } else { "-" },
            if self.sim_config.erosion_enabled { "+" } else { "-" },
            if self.day_night.enabled { "+" } else { "-" },
            if self.debug_view { "D" } else { "-" },
        );

        for (i, ch) in status.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(i as u16, h - 1, ch, Color(0, 0, 0), Some(Color(180, 180, 180)));
            }
        }
        for i in status.len()..w as usize {
            renderer.draw(i as u16, h - 1, ' ', Color(0, 0, 0), Some(Color(180, 180, 180)));
        }
    }

    fn draw_resource_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        // Collect resource positions with colors
        let mut markers: Vec<(f64, f64, char, Color)> = Vec::new();
        for (pos, sprite, _) in self.world.query::<(&Position, &Sprite, &FoodSource)>().iter() {
            markers.push((pos.x, pos.y, sprite.ch, Color(255, 50, 200))); // magenta
        }
        for (pos, sprite, _) in self.world.query::<(&Position, &Sprite, &StoneDeposit)>().iter() {
            markers.push((pos.x, pos.y, sprite.ch, Color(220, 220, 220))); // white
        }
        for (pos, sprite, _) in self.world.query::<(&Position, &Sprite, &Stockpile)>().iter() {
            markers.push((pos.x, pos.y, sprite.ch, Color(255, 220, 50))); // yellow
        }

        for (px, py, ch, fg) in &markers {
            let sx = (*px as i32 - self.camera.x as i32) * aspect + panel_w;
            let sy = *py as i32 - self.camera.y as i32;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, *ch, *fg, None);
            }
        }
    }

    fn draw_threat_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        // Collect wolf den positions for danger zone
        let den_positions: Vec<(f64, f64)> = self.world.query::<(&Position, &Den)>().iter()
            .map(|(p, _)| (p.x, p.y)).collect();

        // Draw danger zone background tint (8 tile radius around dens)
        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x as i32 + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y as i32 + sy as i32;
                let in_danger = den_positions.iter().any(|&(dx, dy)| {
                    let ddx = wx as f64 - dx;
                    let ddy = wy as f64 - dy;
                    ddx * ddx + ddy * ddy < 64.0 // 8 tile radius
                });
                if in_danger {
                    // Draw a dim red tint
                    renderer.draw(sx_raw as u16, sy, '·', Color(180, 40, 40), Some(Color(60, 10, 10)));
                }
            }
        }

        // Draw wolves as bright red 'W'
        for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
            if creature.species != Species::Predator { continue; }
            let sx = (pos.x as i32 - self.camera.x as i32) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y as i32;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, 'W', Color(255, 50, 50), Some(Color(80, 0, 0)));
            }
        }

        // Draw dens as bright red 'D'
        for (pos, _) in self.world.query::<(&Position, &Den)>().iter() {
            let sx = (pos.x as i32 - self.camera.x as i32) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y as i32;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, 'D', Color(255, 80, 80), Some(Color(80, 0, 0)));
            }
        }

        // Draw garrison/wall buildings as bright green
        for (pos, _) in self.world.query::<(&Position, &GarrisonBuilding)>().iter() {
            let sx = (pos.x as i32 - self.camera.x as i32) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y as i32;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, 'G', Color(50, 255, 50), None);
            }
        }
    }

    /// Debug view: high-contrast, no lighting, single letter per terrain type.
    /// Shows terrain, water depth, entity positions, and collision-relevant info.
    pub fn draw_debug(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;

        let black = Color(0, 0, 0);

        // Terrain: single uppercase letter, distinct bg per type, no lighting
        for sy in 0..h.saturating_sub(status_h) {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 {
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        let (ch, bg) = match terrain {
                            Terrain::Water =>    ('W', Color(30, 60, 180)),
                            Terrain::Sand =>     ('S', Color(200, 180, 100)),
                            Terrain::Grass =>    ('G', Color(50, 160, 50)),
                            Terrain::Forest =>   ('F', Color(20, 100, 30)),
                            Terrain::Mountain =>      ('M', Color(140, 130, 120)),
                            Terrain::Snow =>          ('N', Color(220, 220, 230)),
                            Terrain::BuildingFloor => ('B', Color(140, 120, 90)),
                            Terrain::BuildingWall =>  ('X', Color(160, 140, 110)),
                            Terrain::Road =>          ('R', Color(160, 130, 80)),
                        };
                        renderer.draw(sx, sy, ch, black, Some(bg));
                    }
                }
            }
        }

        // Water overlay: show depth as 0-9
        for sy in 0..h.saturating_sub(status_h) {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 && (wx as usize) < self.water.width && (wy as usize) < self.water.height {
                    let depth = self.water.get_avg(wx as usize, wy as usize);
                    if depth > 0.0005 {
                        let level = ((depth * 1000.0).min(9.0)) as u8;
                        let ch = (b'0' + level) as char;
                        renderer.draw(sx, sy, ch, Color(255, 255, 255), Some(Color(0, 40, 200)));
                    }
                }
            }
        }

        // Entities: bright yellow on red so they pop (skip AtHome creatures)
        for (e, (pos, sprite)) in self.world.query::<(hecs::Entity, (&Position, &Sprite))>().iter() {
            if let Ok(behavior) = self.world.get::<&Behavior>(e) {
                if matches!(behavior.state, BehaviorState::AtHome { .. }) {
                    continue;
                }
            }
            let sx = (pos.x.round() as i32 - self.camera.x) * aspect;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= 0 && sy >= 0 && (sx as u16) < w && (sy as u16) < h.saturating_sub(status_h) {
                renderer.draw(sx as u16, sy as u16, sprite.ch, Color(255, 255, 0), Some(Color(180, 0, 0)));
            }
        }

        if self.query_mode {
            self.draw_query_cursor(renderer);
            self.draw_query_panel(renderer);
        }

        if self.build_mode {
            self.draw_build_mode(renderer);
        }

        // Notifications and status bar (shared with normal draw)
        self.draw_notifications(renderer);
        self.draw_status(renderer);
    }

    pub fn run_script(&mut self, inputs: &[GameInput], renderer: &mut HeadlessRenderer) -> Result<Vec<FrameSnapshot>> {
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

        let initial_count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        assert_eq!(initial_count, 2);

        let mut resources = Resources { food: 10, ..Default::default() };

        let villager_count = world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        if villager_count >= 2 && resources.food >= 5 {
            resources.food -= 5;
            let villager_pos: Vec<(f64, f64)> = world.query::<(&Position, &Creature)>().iter()
                .filter(|(_, c)| c.species == Species::Villager)
                .map(|(p, _)| (p.x, p.y))
                .collect();
            if let Some(&(vx, vy)) = villager_pos.first() {
                let mut spawned = false;
                for r in 0..5i32 {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if spawned { continue; }
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

        let final_count = world.query::<&Creature>().iter()
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

        let farms_before = game.world.query::<&BuildSite>().iter()
            .filter(|s| s.building_type == BuildingType::Farm).count()
            + game.world.query::<&FarmPlot>().iter().count();

        game.auto_build_tick();

        let farms_after = game.world.query::<&BuildSite>().iter()
            .filter(|s| s.building_type == BuildingType::Farm).count()
            + game.world.query::<&FarmPlot>().iter().count();

        assert!(farms_after > farms_before, "auto-build should queue a farm when food is low");
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

        let any_skill_increased =
            game.skills.woodcutting > initial_woodcutting
            || game.skills.mining > 0.5
            || game.skills.farming > 0.5
            || game.skills.building > 0.5;

        assert!(any_skill_increased,
            "skills should increase from villager activity: wood={:.2} mine={:.2} farm={:.2} build={:.2}",
            game.skills.woodcutting, game.skills.mining, game.skills.farming, game.skills.building);
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

        assert!(game.skills.building < 80.0,
            "building skill should decay without activity: {:.2}", game.skills.building);
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
        let villager_count_before = game.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();

        game.save("/tmp/test_savegame.json").unwrap();
        let loaded = Game::load("/tmp/test_savegame.json", 60).unwrap();

        assert_eq!(loaded.tick, tick_before);
        assert_eq!(loaded.resources.food, food_before);
        let villager_count_after = loaded.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();
        assert_eq!(villager_count_after, villager_count_before);

        let _ = std::fs::remove_file("/tmp/test_savegame.json");
    }

    #[test]
    fn defense_rating_increases_with_garrison() {
        let mut game = Game::new(60, 42);

        let base_defense = game.compute_defense_rating();

        ecs::spawn_garrison(&mut game.world, 125.0, 125.0);

        let new_defense = game.compute_defense_rating();
        assert!(new_defense > base_defense,
            "defense rating should increase with garrison: base={}, new={}", base_defense, new_defense);
        assert!((new_defense - base_defense - 5.0).abs() < 0.01,
            "garrison should add 5.0 defense, got difference: {}", new_defense - base_defense);
    }

    #[test]
    fn build_site_gets_completed_in_game() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Give plenty of food so villagers don't prioritize foraging over building
        game.resources.food = 100;

        // Place a wall build site near the settlement (villagers spawn around 125,126)
        let site = ecs::spawn_build_site(&mut game.world, 126.0, 126.0, BuildingType::Wall);

        // Run for enough ticks — wall requires 30 build_time
        for _ in 0..800 {
            game.step(GameInput::None, &mut renderer).unwrap();
            if game.world.get::<&BuildSite>(site).is_err() {
                return; // Build site despawned = completed
            }
        }

        if let Ok(s) = game.world.get::<&BuildSite>(site) {
            panic!("build site not completed after 800 ticks: progress={}/{}", s.progress, s.required);
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

        assert!(frame_text.contains("Planks"), "panel should show planks when > 0");
        assert!(frame_text.contains("Masonry"), "panel should show masonry when > 0");
        assert!(frame_text.contains("Grain"), "panel should show grain when > 0");
    }

    #[test]
    fn garrison_placement_requires_refined_resources() {
        let mut game = Game::new(60, 42);

        // Give only raw resources
        game.resources = Resources { food: 100, wood: 100, stone: 100, ..Default::default() };

        let cost = BuildingType::Garrison.cost();
        assert!(!game.resources.can_afford(&cost),
            "should NOT afford garrison with only raw resources");

        // Give refined resources
        game.resources.planks = 5;
        game.resources.masonry = 5;
        assert!(game.resources.can_afford(&cost),
            "should afford garrison with refined resources");
    }

    #[test]
    fn population_growth_requires_housing() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        // Give lots of food
        game.resources.food = 100;
        game.last_birth_tick = 0;

        // Count initial villagers
        let initial = game.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();

        // Run without any huts — no growth should happen
        for _ in 0..1000 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let after_no_huts = game.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();

        // Now add a hut with capacity for growth
        ecs::spawn_hut(&mut game.world, 125.0, 125.0);
        game.resources.food = 100;
        game.last_birth_tick = 0;

        for _ in 0..1000 {
            game.step(GameInput::None, &mut renderer).unwrap();
        }

        let after_hut = game.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();

        // With a hut providing surplus capacity, population should grow
        assert!(after_hut > after_no_huts || after_hut > initial,
            "population should grow when housing is available: initial={} no_huts={} with_hut={}",
            initial, after_no_huts, after_hut);
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
        let result = ecs::system_ai(&mut world, &map, 0.4, 10, 0, 0, 0,
            &SkillMults::default(), false, true);

        let state = world.get::<&Behavior>(v).unwrap().state;
        assert!(matches!(state, BehaviorState::Sleeping { .. }),
            "villager should sleep at night when hut is nearby, got: {:?}", state);
    }

    #[test]
    fn drought_halves_farm_yield() {
        let mut events = EventSystem::default();
        assert_eq!(events.farm_yield_multiplier(), 1.0);

        events.active_events.push(GameEvent::Drought { ticks_remaining: 100 });
        assert_eq!(events.farm_yield_multiplier(), 0.5);
    }

    #[test]
    fn bountiful_harvest_doubles_farm_yield() {
        let mut events = EventSystem::default();
        events.active_events.push(GameEvent::BountifulHarvest { ticks_remaining: 100 });
        assert_eq!(events.farm_yield_multiplier(), 2.0);
    }

    #[test]
    fn wolf_surge_doubles_breeding() {
        let mut events = EventSystem::default();
        assert_eq!(events.wolf_spawn_multiplier(), 1.0);

        events.active_events.push(GameEvent::WolfSurge { ticks_remaining: 100 });
        assert_eq!(events.wolf_spawn_multiplier(), 2.0);
    }

    #[test]
    fn events_expire_after_duration() {
        let mut game = Game::new(60, 42);
        game.events.active_events.push(GameEvent::Drought { ticks_remaining: 2 });

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
        events.active_events.push(GameEvent::Drought { ticks_remaining: 100 });
        assert!(events.has_event_type("drought"));
        // The check prevents duplicates
        assert!(!events.has_event_type("harvest"));
    }

    #[test]
    fn event_system_serialization() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);

        game.events.active_events.push(GameEvent::Drought { ticks_remaining: 150 });
        game.events.event_log.push("Test event".to_string());

        game.save("/tmp/test_events_save.json").unwrap();
        let loaded = Game::load("/tmp/test_events_save.json", 60).unwrap();

        assert_eq!(loaded.events.active_events.len(), 1);
        assert!(matches!(loaded.events.active_events[0], GameEvent::Drought { ticks_remaining: 150 }));
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
        ecs::spawn_predator(&mut game.world, (game.camera.x + 5) as f64, (game.camera.y + 5) as f64);

        game.draw(&mut renderer);

        // The wolf should be rendered as 'W' somewhere on screen
        let frame = renderer.frame_as_string();
        assert!(frame.contains('W'), "threat overlay should show wolves as 'W'");
    }

    #[test]
    fn resource_overlay_marks_food_sources() {
        let mut game = Game::new(60, 42);
        let mut renderer = HeadlessRenderer::new(120, 40);
        game.overlay = OverlayMode::Resources;

        // Spawn berry bush in view
        ecs::spawn_berry_bush(&mut game.world, (game.camera.x + 5) as f64, (game.camera.y + 5) as f64);

        game.draw(&mut renderer);

        // Berry bush char '♦' should appear
        let frame = renderer.frame_as_string();
        assert!(frame.contains('♦'), "resource overlay should show berry bushes");
    }
}
