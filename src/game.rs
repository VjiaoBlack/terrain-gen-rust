use anyhow::Result;
use hecs::World;
use serde::Serialize;

use crate::ecs::{self, Behavior, BehaviorState, Creature, Position, Species, Sprite, FoodSource, Den};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::simulation::{DayNightCycle, MoistureMap, SimConfig, VegetationMap, WaterMap};
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
    Drain,
    None,
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

        Self {
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
        }
    }

    pub fn step(&mut self, input: GameInput, renderer: &mut dyn Renderer) -> Result<()> {
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
            GameInput::Drain => self.water.drain(),
            GameInput::Quit | GameInput::None => {}
        }

        let (vw, vh) = renderer.size();
        // World-space viewport: screen width is divided by aspect ratio
        let world_vw = (vw as i32 / CELL_ASPECT) as u16;
        self.camera.clamp(self.map.width, self.map.height, world_vw, vh);

        // update simulation (skip when paused)
        if !self.paused {
            self.tick += 1;
            ecs::system_hunger(&mut self.world);
            ecs::system_ai(&mut self.world, &self.map);
            ecs::system_movement(&mut self.world, &self.map);

            if self.raining {
                self.water.rain(&self.sim_config);
            }
            // Only run expensive water sim when there's actually water
            if self.raining || self.water.has_water() {
                self.water.update(&mut self.heights, &self.sim_config);
                self.moisture.update(&self.water, &mut self.vegetation, &self.map);
            }

            // rebuild tiles if erosion changed heights
            if self.sim_config.erosion_enabled {
                terrain_gen::rebuild_tiles(&mut self.map, &self.heights, &self.terrain_config);
            }

            // advance day/night cycle and compute Blinn-Phong lighting + shadows (viewport only)
            self.day_night.tick();
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
        renderer.flush()?;
        Ok(())
    }

    pub fn step_headless(&mut self, input: GameInput, renderer: &mut HeadlessRenderer) -> Result<FrameSnapshot> {
        self.step(input, renderer)?;
        Ok(self.snapshot(renderer))
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

        self.draw_status(renderer);
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
                    };
                    lines.push(format!("state: {}", state_str));
                    lines.push(format!("speed: {:.2}", behavior.speed));
                }
                if self.world.get::<&FoodSource>(e).is_ok() {
                    lines.push("Food Source".to_string());
                }
                if self.world.get::<&Den>(e).is_ok() {
                    lines.push("Den (safe zone)".to_string());
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

    fn draw_status(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let rain_str = if self.raining { "ON" } else { "off" };
        let erosion_str = if self.sim_config.erosion_enabled { "ON" } else { "off" };
        let dn_str = if self.day_night.enabled {
            format!("ON {}", self.day_night.time_string())
        } else {
            "off".to_string()
        };
        let view_str = if self.debug_view { "DEBUG" } else { "normal" };
        let fps_str = match self.display_fps {
            Some(fps) => format!("{}", fps),
            None => "---".to_string(),
        };
        let query_str = if self.query_mode { "ON (wasd, q:exit)" } else { "off" };
        let pause_str = if self.paused { "PAUSED" } else { "" };
        let status1 = format!(
            " tick: {}  cam: ({},{})  fps: {}  {}  rain: [r] {}  erosion: [e] {}  time: [t] {}  view: [v] {}  query: [k] {}  pause: [space]  drain: [d]  q: quit ",
            self.tick, self.camera.x, self.camera.y, fps_str, pause_str, rain_str, erosion_str, dn_str, view_str, query_str,
        );
        let status2 = format!(
            " arrows: scroll  ",
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
                            Terrain::Mountain => ('M', Color(140, 130, 120)),
                            Terrain::Snow =>     ('N', Color(220, 220, 230)),
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

        // Status bar (shared with normal draw)
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
