use super::{
    CELL_ASPECT, Color, Den, FoodSource, GarrisonBuilding, PANEL_WIDTH, ROAD_TRAFFIC_THRESHOLD,
    Renderer, ResourceType, Sprite, Stockpile, StoneDeposit, TownHallBuilding,
};
use crate::ecs::{Creature, Position, Species};

impl super::super::Game {
    pub(in super::super) fn draw_resource_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        // Collect resource positions with colors
        let mut markers: Vec<(f64, f64, char, Color)> = Vec::new();
        for (pos, sprite, _) in self
            .world
            .query::<(&Position, &Sprite, &FoodSource)>()
            .iter()
        {
            markers.push((pos.x, pos.y, sprite.ch, Color(255, 50, 200))); // magenta
        }
        for (pos, sprite, _) in self
            .world
            .query::<(&Position, &Sprite, &StoneDeposit)>()
            .iter()
        {
            markers.push((pos.x, pos.y, sprite.ch, Color(220, 220, 220))); // white
        }
        for (pos, sprite, _) in self
            .world
            .query::<(&Position, &Sprite, &Stockpile)>()
            .iter()
        {
            markers.push((pos.x, pos.y, sprite.ch, Color(255, 220, 50))); // yellow
        }

        for (px, py, ch, fg) in &markers {
            if !self.exploration.is_revealed(*px as usize, *py as usize) {
                continue;
            }
            let sx = (*px as i32 - self.camera.x) * aspect + panel_w;
            let sy = *py as i32 - self.camera.y;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, *ch, *fg, None);
            }
        }
    }

    pub(in super::super) fn draw_threat_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        // Layer 1 & 3: Background tints — wolf territory (red-brown) and garrison
        // coverage (green). These compose on top of the already-rendered terrain.
        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                let ux = wx as usize;
                let uy = wy as usize;
                if !self.exploration.is_revealed(ux, uy) {
                    continue;
                }

                let wolf = self.threat_map.wolf_at(ux, uy);
                let garrison = self.threat_map.garrison_at(ux, uy);

                if wolf <= 0.0 && garrison <= 0.0 {
                    continue;
                }

                if let Some(cell) = renderer.get_cell(sx_raw as u16, sy) {
                    let bg = cell.bg.unwrap_or(Color(0, 0, 0));
                    let mut r = bg.0 as f64;
                    let mut g = bg.1 as f64;
                    let mut b = bg.2 as f64;

                    // Wolf territory: tint toward dark red-brown (60, 15, 15)
                    if wolf > 0.0 {
                        let alpha = (wolf * 0.25).min(0.30) as f64;
                        r = r * (1.0 - alpha) + 60.0 * alpha;
                        g = g * (1.0 - alpha) + 15.0 * alpha;
                        b = b * (1.0 - alpha) + 15.0 * alpha;
                    }

                    // Garrison coverage: tint toward green (20, 80, 30)
                    if garrison > 0.0 {
                        let alpha = (garrison * 0.08).min(0.20) as f64;
                        r = r * (1.0 - alpha) + 20.0 * alpha;
                        g = g * (1.0 - alpha) + 80.0 * alpha;
                        b = b * (1.0 - alpha) + 30.0 * alpha;
                    }

                    let tinted = Color(r as u8, g as u8, b as u8);
                    renderer.draw(sx_raw as u16, sy, cell.ch, cell.fg, Some(tinted));
                }
            }
        }

        // Layer 2: Approach corridor markers — amber arrows on undefended chokepoint
        // tiles, and `?` at undefended chokepoints suggesting garrison placement.
        for loc in &self.chokepoint_map.locations {
            let sx = (loc.x as i32 - self.camera.x) * aspect + panel_w;
            let sy = loc.y as i32 - self.camera.y;
            if sx < panel_w || sx >= w as i32 || sy < 0 || sy >= (h - status_h) as i32 {
                continue;
            }
            if !self.exploration.is_revealed(loc.x, loc.y) {
                continue;
            }
            let garrison_cov = self.threat_map.garrison_at(loc.x, loc.y);
            if garrison_cov < 0.3 {
                // Undefended chokepoint — show amber `?` suggesting garrison placement
                renderer.draw(
                    sx as u16,
                    sy as u16,
                    '?',
                    Color(100, 200, 100),
                    Some(Color(30, 50, 20)),
                );
            } else {
                // Defended chokepoint — show dim green marker
                renderer.draw(sx as u16, sy as u16, '+', Color(50, 180, 60), None);
            }
        }

        // Layer 5: Exposure gap markers — `!` at tiles with high exposure along
        // the settlement edge (high threat, low garrison coverage).
        let (scx, scy) = self.settlement_center();
        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                let ux = wx as usize;
                let uy = wy as usize;
                if !self.exploration.is_revealed(ux, uy) {
                    continue;
                }
                let exposure = self.threat_map.exposure_at(ux, uy);
                if exposure < 0.3 {
                    continue;
                }
                // Only show markers within 30-tile radius of settlement
                let dx = wx - scx;
                let dy = wy - scy;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > 900 {
                    continue; // > 30 tiles
                }
                renderer.draw(
                    sx_raw as u16,
                    sy,
                    '!',
                    Color(220, 160, 40),
                    Some(Color(60, 40, 10)),
                );
            }
        }

        // Layer 4: Danger scent intensity — dim red dots where danger scent is high
        // (active wolf presence even without ThreatMap territory marking).
        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                let ux = wx as usize;
                let uy = wy as usize;
                if !self.exploration.is_revealed(ux, uy) {
                    continue;
                }
                let scent = self.danger_scent.get(ux, uy);
                if scent > 0.5 {
                    renderer.draw(
                        sx_raw as u16,
                        sy,
                        '·',
                        Color(180, 40, 40),
                        Some(Color(60, 10, 10)),
                    );
                }
            }
        }

        // Layer 6: Entity markers — wolves, dens, garrisons, town halls (on top)

        // Draw wolves as bright red 'W'
        for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
            if creature.species != Species::Predator {
                continue;
            }
            let sx = (pos.x as i32 - self.camera.x) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(
                    sx as u16,
                    sy as u16,
                    'W',
                    Color(255, 50, 50),
                    Some(Color(80, 0, 0)),
                );
            }
        }

        // Draw dens as bright red 'D'
        for (pos, _) in self.world.query::<(&Position, &Den)>().iter() {
            let sx = (pos.x as i32 - self.camera.x) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(
                    sx as u16,
                    sy as u16,
                    'D',
                    Color(255, 80, 80),
                    Some(Color(80, 0, 0)),
                );
            }
        }

        // Draw garrison buildings as bright green 'G'
        for (pos, _) in self.world.query::<(&Position, &GarrisonBuilding)>().iter() {
            let sx = (pos.x as i32 - self.camera.x) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, 'G', Color(50, 255, 50), None);
            }
        }

        // Draw town halls as bright yellow 'H'
        for (pos, _) in self.world.query::<(&Position, &TownHallBuilding)>().iter() {
            let sx = (pos.x as i32 - self.camera.x) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, 'H', Color(255, 220, 60), None);
            }
        }
    }

    pub(in super::super) fn draw_traffic_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                if !self.exploration.is_revealed(wx as usize, wy as usize) {
                    continue;
                }
                let traffic = self.traffic.get(wx as usize, wy as usize);
                if traffic > 1.0 {
                    let intensity = (traffic / ROAD_TRAFFIC_THRESHOLD).min(1.0);

                    // Color by dominant resource type if available
                    let (r, g, b) = if let Some(rt) =
                        self.traffic.get_dominant_resource(wx as usize, wy as usize)
                    {
                        // Resource-typed coloring, scaled by intensity
                        let (base_r, base_g, base_b) = match rt {
                            ResourceType::Wood => (160.0, 100.0, 40.0),
                            ResourceType::Stone => (160.0, 160.0, 170.0),
                            ResourceType::Food => (60.0, 180.0, 60.0),
                            ResourceType::Grain => (200.0, 180.0, 60.0),
                            ResourceType::Planks => (180.0, 140.0, 60.0),
                            ResourceType::Masonry => (180.0, 180.0, 200.0),
                        };
                        (
                            (base_r * (0.4 + 0.6 * intensity)) as u8,
                            (base_g * (0.4 + 0.6 * intensity)) as u8,
                            (base_b * (0.4 + 0.6 * intensity)) as u8,
                        )
                    } else {
                        // Default amber heat coloring (no resource info)
                        (
                            (80.0 + 175.0 * intensity) as u8,
                            (60.0 + 140.0 * intensity) as u8,
                            (10.0 + 20.0 * intensity) as u8,
                        )
                    };

                    let ch = if traffic >= ROAD_TRAFFIC_THRESHOLD {
                        '='
                    } else if traffic >= 150.0 {
                        // Use oriented trail character for high-traffic sub-road paths
                        self.traffic.trail_char(wx as usize, wy as usize)
                    } else {
                        '·'
                    };
                    renderer.draw(
                        sx_raw as u16,
                        sy,
                        ch,
                        Color(r, g, b),
                        Some(Color(40, 30, 5)),
                    );
                }
            }
        }
    }

    /// Draw wind overlay: arrows showing direction, color intensity showing speed.
    pub(in super::super) fn draw_wind_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                let ux = wx as usize;
                let uy = wy as usize;
                if ux >= self.wind.width || uy >= self.wind.height {
                    continue;
                }
                if !self.exploration.is_revealed(ux, uy) {
                    continue;
                }

                let (vx, vy) = self.wind.get_wind(ux, uy);
                let speed = self.wind.get_speed(ux, uy);
                let shadow = self.wind.get_shadow(ux, uy);

                // Direction arrow: points where wind is going
                // In our coordinate system: +x = right, +y = down
                let ch = if speed < 0.05 {
                    '·' // calm
                } else {
                    // Simple: pick arrow based on dominant axis
                    if vx.abs() > vy.abs() * 1.5 {
                        if vx > 0.0 { '→' } else { '←' }
                    } else if vy.abs() > vx.abs() * 1.5 {
                        if vy > 0.0 { '↓' } else { '↑' }
                    } else {
                        // Diagonal
                        match (vx > 0.0, vy > 0.0) {
                            (true, true) => '↘',
                            (true, false) => '↗',
                            (false, true) => '↙',
                            (false, false) => '↖',
                        }
                    }
                };

                // Color: cyan for wind, intensity by speed. Shadow reduces brightness.
                let intensity = (speed / 1.0).min(1.0) * shadow;
                let r = (30.0 * intensity) as u8;
                let g = (120.0 + 135.0 * intensity) as u8;
                let b = (180.0 + 75.0 * intensity) as u8;

                // Background tint: dark for shadow, lighter for exposed
                let bg_val = (20.0 + 30.0 * shadow) as u8;

                renderer.draw(
                    sx_raw as u16,
                    sy,
                    ch,
                    Color(r, g, b),
                    Some(Color(bg_val, bg_val, bg_val + 10)),
                );
            }
        }
    }

    /// Draw height overlay: grayscale showing raw heightmap values.
    /// Black = water_level (0.0), white = max height (1.0).
    /// Useful for diagnosing erosion artifacts.
    pub(in super::super) fn draw_height_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                let ux = wx as usize;
                let uy = wy as usize;
                if ux >= self.map.width || uy >= self.map.height {
                    continue;
                }

                let idx = uy * self.map.width + ux;
                let height = if idx < self.heights.len() {
                    self.heights[idx]
                } else {
                    0.0
                };

                // Map height to grayscale: 0.0=black, 1.0=white
                // Water level gets blue tint
                let water_level = self.terrain_config.water_level;
                let (ch, fg, bg) = if height <= water_level {
                    // Water: blue, darker = deeper
                    let depth = (water_level - height) / water_level;
                    let b = (80.0 + 120.0 * (1.0 - depth)) as u8;
                    ('~', Color(20, 40, b), Color(10, 20, b / 2))
                } else {
                    // Land: grayscale, with value shown as character
                    let t = ((height - water_level) / (1.0 - water_level)).clamp(0.0, 1.0);
                    let v = (t * 255.0) as u8;
                    // Use block characters for density visualization
                    let ch = match (t * 8.0) as u32 {
                        0 => '.',
                        1 => ':',
                        2 => '-',
                        3 => '=',
                        4 => '+',
                        5 => '#',
                        6 => '%',
                        _ => '@',
                    };
                    (ch, Color(v, v, v), Color(v / 4, v / 4, v / 4))
                };

                renderer.draw(sx_raw as u16, sy, ch, fg, Some(bg));
                // Fill aspect ratio gap
                for dx in 1..aspect {
                    let fill_x = sx_raw + dx as i32;
                    if fill_x >= 0 && (fill_x as u16) < w {
                        renderer.draw(fill_x as u16, sy, ' ', fg, Some(bg));
                    }
                }
            }
        }
    }
}
