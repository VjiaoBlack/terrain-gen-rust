use anyhow::Result;
use hecs::World;
use serde::Serialize;

use crate::ecs::{self, Position, Sprite};
use crate::headless_renderer::HeadlessRenderer;
use crate::renderer::{Cell, Color, Renderer};
use crate::terrain_gen::{self, TerrainGenConfig};
use crate::tilemap::{self, Camera, TileMap};

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
    None,
}

pub struct Game {
    pub target_fps: u32,
    pub tick: u64,
    pub map: TileMap,
    pub camera: Camera,
    pub world: World,
    pub scroll_speed: i32,
}

impl Game {
    pub fn new(target_fps: u32, seed: u32) -> Self {
        let config = TerrainGenConfig { seed, ..Default::default() };
        let map = terrain_gen::generate_terrain(256, 256, &config);
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
            camera,
            world,
            scroll_speed: 2,
        }
    }

    pub fn step(&mut self, input: GameInput, renderer: &mut dyn Renderer) -> Result<()> {
        // input
        match input {
            GameInput::ScrollUp => self.camera.y -= self.scroll_speed,
            GameInput::ScrollDown => self.camera.y += self.scroll_speed,
            GameInput::ScrollLeft => self.camera.x -= self.scroll_speed,
            GameInput::ScrollRight => self.camera.x += self.scroll_speed,
            GameInput::Quit | GameInput::None => {}
        }

        let (vw, vh) = renderer.size();
        self.camera.clamp(self.map.width, self.map.height, vw, vh);

        // update
        self.tick += 1;
        ecs::system_movement(&mut self.world);

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

        // draw terrain
        tilemap::render_map(&self.map, &self.camera, renderer);

        // draw entities (offset by camera)
        for (pos, sprite) in self.world.query::<(&Position, &Sprite)>().iter() {
            let sx = pos.x.round() as i32 - self.camera.x;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= 0 && sy >= 0 && (sx as u16) < w && (sy as u16) < h.saturating_sub(1) {
                renderer.draw(sx as u16, sy as u16, sprite.ch, sprite.fg, None);
            }
        }

        // status line at bottom
        let status = format!(
            " tick: {}  cam: ({},{})  arrows: scroll  q: quit ",
            self.tick, self.camera.x, self.camera.y
        );
        for (i, ch) in status.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(i as u16, h - 1, ch, Color(0, 0, 0), Some(Color(200, 200, 200)));
            }
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
