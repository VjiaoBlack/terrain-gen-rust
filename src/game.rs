use anyhow::Result;
use hecs::World;
use serde::Serialize;

use crate::ecs::{self, Position, Sprite};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::simulation::{DayNightCycle, MoistureMap, SimConfig, VegetationMap, WaterMap};
use crate::terrain_gen::{self, TerrainGenConfig};
use crate::tilemap::{Camera, TileMap};

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
    Drain,
    None,
}

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
}

impl Game {
    pub fn new(target_fps: u32, seed: u32) -> Self {
        let terrain_config = TerrainGenConfig { seed, ..Default::default() };
        let (map, heights) = terrain_gen::generate_terrain(256, 256, &terrain_config);
        let water = WaterMap::new(256, 256);
        let moisture = MoistureMap::new(256, 256);
        let vegetation = VegetationMap::new(256, 256);
        let camera = Camera::new(100, 100);
        let mut world = World::new();

        // spawn a player entity in the center of the map
        ecs::spawn_entity(&mut world, 128.0, 128.0, 0.0, 0.0, '@', Color(255, 255, 0));

        // spawn a few wandering NPCs
        ecs::spawn_entity(&mut world, 110.0, 105.0, 0.1, 0.05, '☺', Color(200, 100, 50));
        ecs::spawn_entity(&mut world, 130.0, 115.0, -0.05, 0.1, '☺', Color(100, 200, 50));

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
            GameInput::Drain => self.water.drain(),
            GameInput::Quit | GameInput::None => {}
        }

        let (vw, vh) = renderer.size();
        self.camera.clamp(self.map.width, self.map.height, vw, vh);

        // update simulation
        self.tick += 1;
        ecs::system_movement(&mut self.world);

        if self.raining {
            self.water.rain(&self.sim_config);
        }
        // Only run expensive water sim when there's actually water
        if self.raining || self.water.has_water() {
            self.water.update(&mut self.heights, &self.sim_config);
            self.moisture.update(&self.water, &mut self.vegetation);
        }

        // rebuild tiles if erosion changed heights
        if self.sim_config.erosion_enabled {
            terrain_gen::rebuild_tiles(&mut self.map, &self.heights, &self.terrain_config);
        }

        // advance day/night cycle and compute Blinn-Phong lighting + shadows (viewport only)
        self.day_night.tick();
        if self.day_night.enabled {
            self.day_night.compute_lighting(
                &self.heights,
                self.map.width,
                self.map.height,
                self.camera.x,
                self.camera.y,
                vw as usize,
                vh as usize,
            );
        }

        // render
        renderer.clear();
        self.draw(renderer);
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

        // draw terrain with day/night lighting
        let (vw, vh) = renderer.size();
        for sy in 0..vh {
            for sx in 0..vw {
                let wx = self.camera.x + sx as i32;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 {
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        let fg = self.day_night.apply_lighting(terrain.fg(), wx as usize, wy as usize);
                        let bg = self.day_night.apply_lighting_bg(terrain.bg(), wx as usize, wy as usize);
                        renderer.draw(sx, sy, terrain.ch(), fg, bg);
                    }
                }
            }
        }

        // draw vegetation on top of terrain (before water)
        for sy in 0..h.saturating_sub(status_h) {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32;
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
                        renderer.draw(sx, sy, ch, fg, None);
                    }
                }
            }
        }

        // draw water on top of terrain
        for sy in 0..h.saturating_sub(status_h) {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 && (wx as usize) < self.water.width && (wy as usize) < self.water.height {
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

        // draw entities (offset by camera)
        for (pos, sprite) in self.world.query::<(&Position, &Sprite)>().iter() {
            let sx = pos.x.round() as i32 - self.camera.x;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= 0 && sy >= 0 && (sx as u16) < w && (sy as u16) < h.saturating_sub(status_h) {
                // Entities get tinted but not shadowed (they're above terrain)
                let (tr, tg, tb) = self.day_night.ambient_tint();
                let fg = Color(
                    (sprite.fg.0 as f64 * tr).clamp(0.0, 255.0) as u8,
                    (sprite.fg.1 as f64 * tg).clamp(0.0, 255.0) as u8,
                    (sprite.fg.2 as f64 * tb).clamp(0.0, 255.0) as u8,
                );
                renderer.draw(sx as u16, sy as u16, sprite.ch, fg, None);
            }
        }

        // status lines at bottom
        let rain_str = if self.raining { "ON" } else { "off" };
        let erosion_str = if self.sim_config.erosion_enabled { "ON" } else { "off" };
        let dn_str = if self.day_night.enabled {
            format!("ON {}", self.day_night.time_string())
        } else {
            "off".to_string()
        };
        let status1 = format!(
            " tick: {}  cam: ({},{})  rain: [r] {}  erosion: [e] {}  time: [t] {}  drain: [d]  q: quit ",
            self.tick, self.camera.x, self.camera.y, rain_str, erosion_str, dn_str,
        );
        let status2 = format!(
            " arrows: scroll  ",
        );

        for (i, ch) in status1.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(i as u16, h - 2, ch, Color(0, 0, 0), Some(Color(200, 200, 200)));
            }
        }
        // fill rest of status line 1
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
