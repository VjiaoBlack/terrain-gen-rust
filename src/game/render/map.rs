use super::{
    CELL_ASPECT, Color, OverlayMode, PANEL_WIDTH, Renderer, Terrain, map_building_center_glyph,
    map_mode_entity_visual,
};
use crate::ecs::{
    self, Behavior, BehaviorState, BuildingType, Creature, Position, Species, Sprite,
};

impl super::super::Game {
    // -----------------------------------------------------------------------
    // Map Mode: flat symbolic rendering (no lighting, no animation).
    // -----------------------------------------------------------------------

    pub fn draw_map_mode(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let panel_w = PANEL_WIDTH;

        // Panel (shared with normal mode)
        self.draw_panel(renderer);

        // --- Half-block minimap: 2x vertical density, 1 char per map tile ---
        // Each terminal cell shows TWO map rows using ▄ (U+2584):
        //   bg color = top tile, fg color = bottom tile.
        // Horizontal: 1 terminal column = 1 map tile (no aspect ratio padding).
        // This gives ~4x more map coverage than normal mode.
        for sy in 0..h.saturating_sub(status_h) {
            for sx in panel_w..w {
                // 2 map rows per terminal row, 1 map col per terminal col
                let wx = self.camera.x + (sx - panel_w) as i32;
                let wy_top = self.camera.y * 2 + (sy as i32) * 2;
                let wy_bot = wy_top + 1;

                let tile_color = |mx: i32, my: i32| -> Color {
                    if mx < 0 || my < 0 || mx >= self.map.width as i32 || my >= self.map.height as i32 {
                        return Color(5, 5, 10); // off-map
                    }
                    let ux = mx as usize;
                    let uy = my as usize;

                    if !self.exploration.is_revealed(ux, uy) {
                        return Color(20, 20, 25);
                    }

                    let i = uy * self.map.width + ux;

                    // Water check: pipe_water depth or Terrain::Water
                    let water_depth = self.pipe_water.get_depth(ux, uy);
                    let is_ocean = matches!(self.map.get(ux, uy), Some(Terrain::Water));
                    if is_ocean || water_depth > 0.01 {
                        let depth = if is_ocean {
                            0.5
                        } else {
                            (water_depth * 4.0).min(1.0)
                        };
                        return Color(
                            (20.0 * (1.0 - depth)) as u8,
                            (50.0 + 60.0 * depth) as u8,
                            (120.0 + 80.0 * depth) as u8,
                        );
                    }

                    // Discharge river tint
                    if i < self.discharge.len() {
                        let d = crate::hydrology::erf_approx(0.4 * self.discharge[i]);
                        if d > 0.2 {
                            let a = d.min(0.9);
                            let terrain_c = self.map.get(ux, uy)
                                .map(|t| t.fg())
                                .unwrap_or(Color(80, 80, 80));
                            return Color(
                                (terrain_c.0 as f64 * (1.0 - a) + 92.0 * a) as u8,
                                (terrain_c.1 as f64 * (1.0 - a) + 133.0 * a) as u8,
                                (terrain_c.2 as f64 * (1.0 - a) + 142.0 * a) as u8,
                            );
                        }
                    }

                    // Terrain color — simple, no lighting
                    self.map.get(ux, uy)
                        .map(|t| t.fg())
                        .unwrap_or(Color(80, 80, 80))
                };

                let top_color = tile_color(wx, wy_top);
                let bot_color = tile_color(wx, wy_bot);

                // ▄ = lower half block: fg = bottom tile, bg = top tile
                renderer.draw(sx, sy, '▄', bot_color, Some(top_color));
            }
        }

        // Skip the old per-tile rendering below — everything is handled above.
        // Entity overlay and status bar still rendered after.
        // (The old code path for overlays/entities follows)
        // Old terrain rendering removed — half-block minimap above replaces it.
        // Entity rendering uses half-block coordinates below.
        let aspect = CELL_ASPECT; // needed for entity position calculations
        // Build sites: show ? marker
        for (pos, _site) in self.world.query::<(&Position, &ecs::BuildSite)>().iter() {
            let sx_i = (pos.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
            let sy_i = pos.y.round() as i32 - self.camera.y;
            if sx_i >= panel_w as i32
                && sy_i >= 0
                && (sx_i as u16) < w
                && (sy_i as u16) < h.saturating_sub(status_h)
            {
                renderer.draw(sx_i as u16, sy_i as u16, '?', Color(200, 180, 100), None);
            }
        }

        // --- Entity pass: bright glyphs, no day/night tinting ---
        for (e, (pos, sprite)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Sprite))>()
            .iter()
        {
            let creature_opt = self.world.get::<&Creature>(e).ok();
            let bstate = self.world.get::<&Behavior>(e).ok().map(|b| b.state);

            // Only creatures with behavior states get map-mode visuals
            let (display_ch, display_fg) =
                if let (Some(creature), Some(bstate_val)) = (creature_opt.as_deref(), &bstate) {
                    match map_mode_entity_visual(creature.species, bstate_val) {
                        Some(vis) => vis,
                        None => continue, // hidden entity (e.g. prey AtHome)
                    }
                } else {
                    // Non-creature entities (food sources, stone deposits, dens)
                    // Render with their normal sprite — they are world objects
                    (sprite.ch, sprite.fg)
                };

            // Hide entities on unexplored tiles
            if !self
                .exploration
                .is_revealed(pos.x.round() as usize, pos.y.round() as usize)
            {
                continue;
            }

            let sx_i = (pos.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
            let sy_i = pos.y.round() as i32 - self.camera.y;
            if sx_i >= panel_w as i32
                && sy_i >= 0
                && (sx_i as u16) < w
                && (sy_i as u16) < h.saturating_sub(status_h)
            {
                // Full brightness — no day/night dimming in Map Mode
                // Hauler brightness: seeking/idle villagers at 70%, haulers at 100%
                let dim = if matches!(
                    bstate,
                    Some(BehaviorState::Seek { .. })
                        | Some(BehaviorState::Wander { .. })
                        | Some(BehaviorState::Idle { .. })
                ) && creature_opt
                    .as_deref()
                    .map_or(false, |c| c.species == Species::Villager)
                {
                    0.7
                } else {
                    1.0
                };
                let fg = Color(
                    (display_fg.0 as f64 * dim).clamp(0.0, 255.0) as u8,
                    (display_fg.1 as f64 * dim).clamp(0.0, 255.0) as u8,
                    (display_fg.2 as f64 * dim).clamp(0.0, 255.0) as u8,
                );
                renderer.draw(sx_i as u16, sy_i as u16, display_ch, fg, None);
            }
        }

        // --- Shared UI overlays (same as Normal mode) ---
        if self.overlay == OverlayMode::Resources {
            self.draw_resource_overlay(renderer);
        } else if self.overlay == OverlayMode::Threats {
            self.draw_threat_overlay(renderer);
        } else if self.overlay == OverlayMode::Traffic {
            self.draw_traffic_overlay(renderer);
        } else if self.overlay == OverlayMode::Wind {
            self.draw_wind_overlay(renderer);
        } else if self.overlay == OverlayMode::Height {
            self.draw_height_overlay(renderer);
        } else if self.overlay == OverlayMode::Discharge {
            self.draw_discharge_overlay(renderer);
        } else if self.overlay == OverlayMode::Moisture {
            self.draw_moisture_overlay(renderer);
        } else if self.overlay == OverlayMode::Slope {
            self.draw_slope_overlay(renderer);
        }

        // WindFlow: particles ARE the visualization — draw on top
        if self.overlay == OverlayMode::WindFlow {
            for p in &self.particles {
                let sx = (p.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
                let sy = p.y.round() as i32 - self.camera.y;
                if sx >= panel_w as i32
                    && sy >= 0
                    && (sx as u16) < w
                    && (sy as u16) < h.saturating_sub(status_h)
                {
                    let age = 1.0 - (p.life as f64 / p.max_life.max(1) as f64);
                    let fade = if age > 0.6 {
                        1.0 - ((age - 0.6) / 0.4)
                    } else {
                        1.0
                    };
                    let fg = Color(
                        (p.fg.0 as f64 * fade) as u8,
                        (p.fg.1 as f64 * fade) as u8,
                        (p.fg.2 as f64 * fade) as u8,
                    );
                    renderer.draw(sx as u16, sy as u16, p.ch, fg, None);
                }
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
        // No weather in Map Mode (design decision: no visual noise)
        self.draw_minimap(renderer);
        self.draw_status(renderer);
    }

    /// Resolve terrain glyph for Map Mode, including vegetation overlay.
    fn map_terrain_glyph(&self, terrain: &Terrain, wx: usize, wy: usize) -> (char, Color, Color) {
        // Base terrain
        let mut ch = terrain.map_ch();
        let mut fg = terrain.map_fg();
        let bg = terrain.map_bg();

        // Vegetation overlay on grass/scrubland tiles
        if matches!(terrain, Terrain::Grass | Terrain::Scrubland | Terrain::Bare) {
            if wx < self.vegetation.width && wy < self.vegetation.height {
                let v = self.vegetation.get(wx, wy);
                if v > 0.8 {
                    ch = '\u{2660}'; // ♠
                    fg = Color(15, 90, 20);
                } else if v > 0.5 {
                    ch = '\u{2663}'; // ♣
                    fg = Color(25, 120, 30);
                } else if v > 0.2 {
                    ch = '\'';
                    fg = Color(50, 150, 50);
                }
                // v <= 0.2: keep base terrain glyph
            }
        }

        (ch, fg, bg)
    }

    /// Draw a building center marker at the building's position.
    fn draw_map_building_marker(
        &self,
        renderer: &mut dyn Renderer,
        pos: &Position,
        bt: &BuildingType,
        aspect: i32,
        panel_w: u16,
    ) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let (bw, bh) = bt.size();
        // Center of building footprint
        let cx = pos.x.round() as i32 + bw / 2;
        let cy = pos.y.round() as i32 + bh / 2;
        let sx_i = (cx - self.camera.x) * aspect + panel_w as i32;
        let sy_i = cy - self.camera.y;
        if sx_i >= panel_w as i32
            && sy_i >= 0
            && (sx_i as u16) < w
            && (sy_i as u16) < h.saturating_sub(status_h)
        {
            let (glyph, color) = map_building_center_glyph(bt);
            // Use building wall bg for contrast
            renderer.draw(
                sx_i as u16,
                sy_i as u16,
                glyph,
                color,
                Some(Terrain::BuildingFloor.map_bg()),
            );
        }
    }
}
