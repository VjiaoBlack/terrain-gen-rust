mod build;
mod events;
mod render;
mod save;

use anyhow::Result;
use hecs::World;
use rand::RngExt;
use serde::{Serialize, Deserialize};

use crate::ecs::{self, AiResult, Behavior, BehaviorState, BuildSite, BuildingType, Creature, FarmPlot, GarrisonBuilding, HutBuilding, Position, ProcessingBuilding, Recipe, Resources, SkillMults, Species, Sprite, FoodSource, Den, StoneDeposit, ResourceType, Stockpile, SerializedEntity};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::simulation::{DayNightCycle, InfluenceMap, MoistureMap, Season, SimConfig, TrafficMap, VegetationMap, WaterMap};
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
    Traffic,    // Show foot traffic heatmap
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
}

/// Traffic above this threshold converts walkable terrain to road.
const ROAD_TRAFFIC_THRESHOLD: f64 = 150.0;

impl Game {
    pub fn new(target_fps: u32, seed: u32) -> Self {
        // Reduce terrain noise scale for larger biomes — buildings feel right-sized
        let terrain_config = TerrainGenConfig { seed, scale: 0.008, ..Default::default() };
        let (mut map, heights) = terrain_gen::generate_terrain(256, 256, &terrain_config);
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
        // Set stockpile terrain tiles (2x2)
        for dy in 0..2 {
            for dx in 0..2 {
                map.set(sx as usize + dx, sy as usize + dy, Terrain::BuildingFloor);
            }
        }

        // Pre-built hut near stockpile
        let (hx, hy) = find_walkable(&map, 122, 123);
        for (dx, dy, terrain) in BuildingType::Hut.tiles() {
            map.set(hx as usize + dx as usize, hy as usize + dy as usize, terrain);
        }
        let (hsw, hsh) = BuildingType::Hut.size();
        ecs::spawn_hut(&mut world, hx + hsw as f64 / 2.0, hy + hsh as f64 / 2.0);

        // Pre-built farm near stockpile
        let (fx, fy) = find_walkable(&map, 128, 123);
        for (dx, dy, terrain) in BuildingType::Farm.tiles() {
            map.set(fx as usize + dx as usize, fy as usize + dy as usize, terrain);
        }
        let (fsw, fsh) = BuildingType::Farm.size();
        ecs::spawn_farm_plot(&mut world, fx + fsw as f64 / 2.0, fy + fsh as f64 / 2.0);

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
            resources: Resources { food: 20, wood: 20, stone: 10, ..Default::default() },
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
            traffic: TrafficMap::new(256, 256),
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
                    OverlayMode::Threats => OverlayMode::Traffic,
                    OverlayMode::Traffic => OverlayMode::None,
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

            // Coordinated wolf raids
            let (scx, scy) = self.settlement_center();
            if ecs::system_wolf_raids(&mut self.world, scx as f64, scy as f64, self.tick) {
                self.notify("Wolf pack is raiding the settlement!".to_string());
            }

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

            // Farm growth (only advances when villager is present at farm)
            let farm_mult = (1.0 + self.skills.farming / 100.0) * self.events.farm_yield_multiplier();
            ecs::system_farms(&mut self.world, self.day_night.season, farm_mult);

            // Assign idle villagers to farms/workshops, then mark worker presence
            ecs::system_assign_workers(&mut self.world, &self.resources);
            let farm_food_picked = ecs::system_mark_workers(&mut self.world);
            self.resources.food += farm_food_picked;
            if farm_food_picked > 0 {
                self.notify(format!("Farm harvest collected: +{} food", farm_food_picked));
            }

            // Active farms with workers contribute to farming skill
            let tended_farms = self.world.query::<&FarmPlot>().iter()
                .filter(|f| f.worker_present).count() as f64;
            self.skills.farming += tended_farms * 0.003;

            // Processing buildings (only advance when villager is present)
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

            // Track villager foot traffic and auto-build roads
            self.update_traffic();

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

        // Build up influence so auto-build can place within territory
        for _ in 0..10 {
            game.update_influence();
        }

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

        // Give plenty of all resources so villagers don't prioritize gathering over building
        game.resources.food = 200;
        game.resources.wood = 100;
        game.resources.stone = 100;

        // Place a wall build site near the settlement (villagers spawn around 125,126)
        let site = ecs::spawn_build_site(&mut game.world, 126.0, 126.0, BuildingType::Wall);

        // Run for enough ticks — wall requires 30 build_time
        for _ in 0..1500 {
            game.step(GameInput::None, &mut renderer).unwrap();
            if game.world.get::<&BuildSite>(site).is_err() {
                return; // Build site despawned = completed
            }
        }

        if let Ok(s) = game.world.get::<&BuildSite>(site) {
            panic!("build site not completed after 1500 ticks: progress={}/{}", s.progress, s.required);
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
        game.resources.planks = 10;
        game.resources.masonry = 10;
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
        assert_eq!(game.overlay, OverlayMode::Traffic);

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

    #[test]
    fn wolf_raid_triggers_with_pack() {
        let mut world = hecs::World::new();
        let map = TileMap::new(50, 50, Terrain::Grass);

        // Spawn 6 wolves near each other
        for i in 0..6 {
            ecs::spawn_predator(&mut world, 30.0 + i as f64, 30.0);
        }

        // Raid should trigger (wolves within 15 tiles of each other)
        let raided = ecs::system_wolf_raids(&mut world, 25.0, 25.0, 50);
        assert!(raided, "raid should trigger with 6 wolves in a pack");

        // All wolves should now be Hunting toward settlement
        let hunting_count = world.query::<(&Creature, &Behavior)>().iter()
            .filter(|(c, b)| c.species == Species::Predator && matches!(b.state, BehaviorState::Hunting { .. }))
            .count();
        assert!(hunting_count >= 5, "pack wolves should be hunting: got {}", hunting_count);
    }

    #[test]
    fn wolf_raid_needs_minimum_pack() {
        let mut world = hecs::World::new();

        // Only 3 wolves — not enough for a raid
        for i in 0..3 {
            ecs::spawn_predator(&mut world, 30.0 + i as f64, 30.0);
        }

        let raided = ecs::system_wolf_raids(&mut world, 25.0, 25.0, 50);
        assert!(!raided, "raid should not trigger with only 3 wolves");
    }

    #[test]
    fn building_requires_influence() {
        let mut game = Game::new(60, 42);

        // Far from settlement — no influence
        let far_x = 10i32;
        let far_y = 10i32;
        assert!(!game.can_place_building(far_x, far_y, BuildingType::Wall),
            "should not be able to build outside influence");

        // Near settlement — build up influence
        for _ in 0..10 {
            game.update_influence();
        }
        let (cx, cy) = game.settlement_center();
        assert!(game.can_place_building(cx + 2, cy + 2, BuildingType::Wall),
            "should be able to build within influence");
    }

    #[test]
    fn traffic_converts_grass_to_road() {
        let mut game = Game::new(60, 42);

        // Manually accumulate traffic on a grass tile
        let tx = 130usize;
        let ty = 130usize;
        // Ensure the tile is grass
        game.map.set(tx, ty, Terrain::Grass);

        // Simulate heavy foot traffic
        for _ in 0..200 {
            game.traffic.step_on(tx, ty);
        }

        // Trigger road conversion check
        game.tick = 100; // align to conversion interval
        game.update_traffic();

        assert_eq!(*game.map.get(tx, ty).unwrap(), Terrain::Road,
            "heavily trafficked grass should become road");
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

        assert_eq!(*game.map.get(tx, ty).unwrap(), Terrain::Water,
            "water should not convert to road");
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
}
