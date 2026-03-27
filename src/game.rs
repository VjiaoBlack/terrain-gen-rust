use anyhow::Result;
use hecs::World;
use serde::Serialize;

use crate::ecs::{self, Behavior, BehaviorState, BuildSite, BuildingType, Creature, FarmPlot, Position, Species, Sprite, FoodSource, Den, StoneDeposit, ResourceType, Stockpile};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::simulation::{DayNightCycle, InfluenceMap, MoistureMap, SimConfig, VegetationMap, WaterMap};
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
    Restart,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct Resources {
    pub food: u32,
    pub wood: u32,
    pub stone: u32,
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
            resources: Resources::default(),
            build_mode: false,
            build_cursor_x: 128,
            build_cursor_y: 128,
            selected_building: BuildingType::Wall,
            influence: InfluenceMap::new(256, 256),
            last_birth_tick: 0,
            notifications: Vec::new(),
            game_over: false,
            peak_population: 3,
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
                    let world_vw = (vw as i32 / CELL_ASPECT) as u16;
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
                    // Center cursor on screen
                    let (vw, vh) = renderer.size();
                    let world_vw = vw as i32 / CELL_ASPECT;
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
                    let world_vw = vw as i32 / CELL_ASPECT;
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
            GameInput::Quit | GameInput::Restart | GameInput::None => {}
        }

        let (vw, vh) = renderer.size();
        // World-space viewport: screen width is divided by aspect ratio
        let world_vw = (vw as i32 / CELL_ASPECT) as u16;
        self.camera.clamp(self.map.width, self.map.height, world_vw, vh);

        // update simulation (skip when paused)
        if !self.paused {
            self.tick += 1;

            // Clean up old notifications
            self.notifications.retain(|(t, _)| self.tick - t < 200);

            // Apply seasonal modifiers
            let mods = self.day_night.season_modifiers();

            ecs::system_hunger(&mut self.world, mods.hunger_mult);
            let (deposits, food_consumed) = ecs::system_ai(&mut self.world, &self.map, mods.wolf_aggression, self.resources.food);
            let mut deposited_food = 0u32;
            let mut deposited_wood = 0u32;
            let mut deposited_stone = 0u32;
            for res in deposits {
                match res {
                    ResourceType::Food => { self.resources.food += 1; deposited_food += 1; },
                    ResourceType::Wood => { self.resources.wood += 1; deposited_wood += 1; },
                    ResourceType::Stone => { self.resources.stone += 1; deposited_stone += 1; },
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
            if food_consumed > 0 {
                self.resources.food = self.resources.food.saturating_sub(food_consumed);
                self.notify(format!("Villager ate from stockpile (-{} food)", food_consumed));
            }

            ecs::system_movement(&mut self.world, &self.map);

            // Count creatures before breeding to detect new spawns
            let prey_before = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Prey).count();
            let wolf_before = self.world.query::<&Creature>().iter()
                .filter(|c| c.species == Species::Predator).count();

            ecs::system_breeding(&mut self.world, self.day_night.season);

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

            // Farm growth and harvest
            let farm_food = ecs::system_farms(&mut self.world, self.day_night.season);
            self.resources.food += farm_food;
            if farm_food > 0 {
                self.notify(format!("Farm harvested: +{} food", farm_food));
            }

            // Check for completed buildings
            self.check_build_completion();

            // Update influence map: villagers emit 1.0, active build sites emit 0.5
            self.update_influence();

            // Population growth check
            self.try_population_growth();

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
        let (cost_f, cost_w, cost_s) = bt.cost();
        if self.resources.food < cost_f || self.resources.wood < cost_w || self.resources.stone < cost_s {
            return;
        }

        // Deduct resources
        self.resources.food -= cost_f;
        self.resources.wood -= cost_w;
        self.resources.stone -= cost_s;

        // Spawn build site entity
        ecs::spawn_build_site(&mut self.world, bx as f64, by as f64, bt);
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
            // Spawn a FarmPlot entity at the center of completed farms
            if site.building_type == BuildingType::Farm {
                let (sw, sh) = site.building_type.size();
                let cx = pos.x + sw as f64 / 2.0;
                let cy = pos.y + sh as f64 / 2.0;
                ecs::spawn_farm_plot(&mut self.world, cx, cy);
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
    fn try_population_growth(&mut self) {
        if self.tick - self.last_birth_tick <= 500 {
            return;
        }

        let villager_count = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager)
            .count();

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

    pub fn draw(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 2u16; // reserve 2 lines for status
        let aspect = CELL_ASPECT;

        // draw terrain with day/night lighting
        // Each world tile occupies `aspect` screen columns for square pixels.
        for sy in 0..h {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 {
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        if *terrain == Terrain::Water {
                            // Water terrain: no day/night shading, constant appearance
                            renderer.draw(sx, sy, terrain.ch(), terrain.fg(), terrain.bg());
                        } else {
                            let fg = self.day_night.apply_lighting(terrain.fg(), wx as usize, wy as usize);
                            let bg = self.day_night.apply_lighting_bg(terrain.bg(), wx as usize, wy as usize);
                            renderer.draw(sx, sy, terrain.ch(), fg, bg);
                        }
                    }
                }
            }
        }

        // draw vegetation on top of terrain (before water)
        for sy in 0..h.saturating_sub(status_h) {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
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
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
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
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
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
            let sx = (pos.x.round() as i32 - self.camera.x) * aspect;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= 0 && sy >= 0 && (sx as u16) < w && (sy as u16) < h.saturating_sub(status_h) {
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
                renderer.draw(sx as u16, sy as u16, sprite.ch, fg, None);
            }
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
        let status_h = 2u16;
        let aspect = CELL_ASPECT;
        let (bw, bh) = self.selected_building.size();

        let valid = self.can_place_building(self.build_cursor_x, self.build_cursor_y, self.selected_building);

        // Draw ghost building footprint
        for dy in 0..bh {
            for dx in 0..bw {
                let wx = self.build_cursor_x + dx;
                let wy = self.build_cursor_y + dy;
                let sx = (wx - self.camera.x) * aspect;
                let sy = wy - self.camera.y;
                if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
                    for ax in 0..aspect {
                        let cx = sx + ax;
                        if cx >= 0 && (cx as u16) < w {
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
        let (cost_f, cost_w, cost_s) = self.selected_building.cost();
        let name = self.selected_building.name();
        let line1 = format!(" BUILD: {} (tab:cycle, enter:place, b/esc:exit) ", name);
        let line2 = format!(" Cost: F:{} W:{} S:{} | Have: F:{} W:{} S:{} ",
            cost_f, cost_w, cost_s, self.resources.food, self.resources.wood, self.resources.stone);
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
        let status_h = 2u16;
        let aspect = CELL_ASPECT;

        let sx = (self.query_cx - self.camera.x) * aspect;
        let sy = self.query_cy - self.camera.y;

        // Draw cursor bracket across aspect-width cells
        if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
            for dx in 0..aspect {
                let cx = sx + dx;
                if cx >= 0 && (cx as u16) < w {
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
        let status_h = 2u16;

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
        let status_h = 2u16;
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
            (&format!("Resources: {} food, {} wood, {} stone",
                self.resources.food, self.resources.wood, self.resources.stone), dim),
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
        let rain_str = if self.raining { "ON" } else { "off" };
        let erosion_str = if self.sim_config.erosion_enabled { "ON" } else { "off" };
        let dn_str = if self.day_night.enabled {
            format!("ON {} {}", self.day_night.time_string(), self.day_night.date_string())
        } else {
            "off".to_string()
        };
        let view_str = if self.debug_view { "DEBUG" } else { "normal" };
        let fps_str = match self.display_fps {
            Some(fps) => format!("{}", fps),
            None => "---".to_string(),
        };
        let query_str = if self.query_mode { "ON (wasd, q:exit)" } else { "off" };
        let build_str = if self.build_mode {
            let (cf, cw, cs) = self.selected_building.cost();
            format!("{} (f:{} w:{} s:{})", self.selected_building.name(), cf, cw, cs)
        } else { "off".to_string() };
        let pause_str = if self.paused { "PAUSED" } else { "" };
        let status1 = format!(
            " tick: {}  cam: ({},{})  fps: {}  {}  rain: [r] {}  erosion: [e] {}  time: [t] {}  view: [v] {}  query: [k] {}  build: [b] {}  pause: [space]  q: quit ",
            self.tick, self.camera.x, self.camera.y, fps_str, pause_str, rain_str, erosion_str, dn_str, view_str, query_str, build_str,
        );
        let villager_count = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Villager).count();
        let prey_count = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Prey).count();
        let wolf_count = self.world.query::<&Creature>().iter()
            .filter(|c| c.species == Species::Predator).count();

        let status2 = format!(
            " arrows: scroll  |  Food: {}  Wood: {}  Stone: {}  |  Pop: {}  Prey: {}  Wolves: {}  |  drain: [d]",
            self.resources.food, self.resources.wood, self.resources.stone,
            villager_count, prey_count, wolf_count,
        );

        for (i, ch) in status1.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(i as u16, h - 2, ch, Color(0, 0, 0), Some(Color(200, 200, 200)));
            }
        }
        for i in status1.len()..w as usize {
            renderer.draw(i as u16, h - 2, ' ', Color(0, 0, 0), Some(Color(200, 200, 200)));
        }

        for (i, ch) in status2.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(i as u16, h - 1, ch, Color(0, 0, 0), Some(Color(170, 170, 170)));
            }
        }
        for i in status2.len()..w as usize {
            renderer.draw(i as u16, h - 1, ' ', Color(0, 0, 0), Some(Color(170, 170, 170)));
        }
    }

    /// Debug view: high-contrast, no lighting, single letter per terrain type.
    /// Shows terrain, water depth, entity positions, and collision-relevant info.
    pub fn draw_debug(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 2u16;
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

        let mut resources = Resources { food: 10, wood: 0, stone: 0 };

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
}
