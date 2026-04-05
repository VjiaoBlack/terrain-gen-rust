use super::{CELL_ASPECT, Color, Renderer, Terrain};
use crate::ecs::{Behavior, BehaviorState, Position, Sprite};

impl super::super::Game {
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
                if wx >= 0
                    && wy >= 0
                    && self.dirty.is_dirty(wx as usize, wy as usize)
                    && let Some(terrain) = self.map.get(wx as usize, wy as usize)
                {
                    let (ch, bg) = match terrain {
                        Terrain::Water => ('W', Color(30, 60, 180)),
                        Terrain::Sand => ('S', Color(200, 180, 100)),
                        Terrain::Grass => ('G', Color(50, 160, 50)),
                        Terrain::Forest => ('F', Color(20, 100, 30)),
                        Terrain::Mountain => ('M', Color(140, 130, 120)),
                        Terrain::Snow => ('N', Color(220, 220, 230)),
                        Terrain::BuildingFloor => ('B', Color(140, 120, 90)),
                        Terrain::BuildingWall => ('X', Color(160, 140, 110)),
                        Terrain::Road => ('R', Color(160, 130, 80)),
                        Terrain::Cliff => ('C', Color(100, 90, 80)),
                        Terrain::Marsh => ('H', Color(60, 120, 80)),
                        Terrain::Desert => ('D', Color(210, 190, 120)),
                        Terrain::Tundra => ('T', Color(180, 190, 200)),
                        Terrain::Scrubland => ('U', Color(140, 150, 80)),
                        Terrain::Stump => ('%', Color(100, 80, 40)),
                        Terrain::Bare => ('.', Color(90, 80, 50)),
                        Terrain::Sapling => ('!', Color(40, 140, 40)),
                        Terrain::Quarry => ('Q', Color(140, 130, 115)),
                        Terrain::QuarryDeep => ('V', Color(110, 100, 90)),
                        Terrain::ScarredGround => ('s', Color(145, 135, 120)),
                        Terrain::Ford => ('~', Color(80, 140, 220)),
                        Terrain::Bridge => ('#', Color(140, 100, 50)),
                        Terrain::Ice => ('=', Color(180, 210, 240)),
                        Terrain::FloodWater => ('~', Color(100, 150, 200)),
                        Terrain::Burning => ('*', Color(255, 120, 20)),
                        Terrain::Scorched => ('`', Color(80, 70, 60)),
                    };
                    renderer.draw(sx, sy, ch, black, Some(bg));
                }
            }
        }

        // Water overlay: show depth as 0-9
        for sy in 0..h.saturating_sub(status_h) {
            for sx in 0..w {
                let wx = self.camera.x + sx as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0
                    && wy >= 0
                    && (wx as usize) < self.state.water.width
                    && (wy as usize) < self.state.water.height
                    && self.dirty.is_dirty(wx as usize, wy as usize)
                {
                    let depth = self.state.water.get_depth(wx as usize, wy as usize);
                    if depth > 0.0005 {
                        let level = ((depth * 1000.0).min(9.0)) as u8;
                        let ch = (b'0' + level) as char;
                        renderer.draw(sx, sy, ch, Color(255, 255, 255), Some(Color(0, 40, 200)));
                    }
                }
            }
        }

        // Entities: bright yellow on red so they pop (skip AtHome creatures)
        for (e, (pos, sprite)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Sprite))>()
            .iter()
        {
            if let Ok(behavior) = self.world.get::<&Behavior>(e)
                && matches!(behavior.state, BehaviorState::AtHome { .. })
            {
                continue;
            }
            let sx = (pos.x.round() as i32 - self.camera.x) * aspect;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= 0 && sy >= 0 && (sx as u16) < w && (sy as u16) < h.saturating_sub(status_h) {
                renderer.draw(
                    sx as u16,
                    sy as u16,
                    sprite.ch,
                    Color(255, 255, 0),
                    Some(Color(180, 0, 0)),
                );
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
}
