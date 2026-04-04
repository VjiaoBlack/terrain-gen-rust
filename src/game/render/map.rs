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
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH;

        // Panel (shared with normal mode)
        self.draw_panel(renderer);

        // --- Terrain pass: flat glyphs, no lighting, no season tint ---
        for sy in 0..h.saturating_sub(status_h) {
            for sx in panel_w..w {
                let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 {
                    // Dirty-rect: skip clean tiles
                    if !self.dirty.is_dirty(wx as usize, wy as usize) {
                        continue;
                    }
                    // Fog of exploration
                    if !self.exploration.is_revealed(wx as usize, wy as usize) {
                        renderer.draw(
                            sx,
                            sy,
                            '\u{2591}', // ░
                            Color(35, 35, 40),
                            Some(Color(12, 12, 15)),
                        );
                        continue;
                    }
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        // Water depth override (pipe_water for physics-based depth)
                        let water_depth = self.pipe_water.get_depth(wx as usize, wy as usize);
                        if water_depth > 0.005
                            && !matches!(
                                terrain,
                                Terrain::Water | Terrain::BuildingFloor | Terrain::BuildingWall
                            )
                        {
                            renderer.draw(
                                sx,
                                sy,
                                '~',
                                Color(60, 120, 220),
                                Some(Color(20, 50, 120)),
                            );
                        } else {
                            let (ch, fg, bg) =
                                self.map_terrain_glyph(terrain, wx as usize, wy as usize);
                            let (ch, fg, bg) =
                                self.worn_terrain_override(wx as usize, wy as usize, ch, fg, bg);
                            renderer.draw(sx, sy, ch, fg, Some(bg));
                        }
                    }
                }
            }
        }

        // --- Building center markers (on top of terrain) ---
        // Completed buildings with marker components:
        for (pos, _stockpile) in self.world.query::<(&Position, &ecs::Stockpile)>().iter() {
            self.draw_map_building_marker(renderer, pos, &BuildingType::Stockpile, aspect, panel_w);
        }
        for (pos, _hut) in self.world.query::<(&Position, &ecs::HutBuilding)>().iter() {
            self.draw_map_building_marker(renderer, pos, &BuildingType::Hut, aspect, panel_w);
        }
        for (pos, _garrison) in self
            .world
            .query::<(&Position, &ecs::GarrisonBuilding)>()
            .iter()
        {
            self.draw_map_building_marker(renderer, pos, &BuildingType::Garrison, aspect, panel_w);
        }
        for (pos, _hall) in self
            .world
            .query::<(&Position, &ecs::TownHallBuilding)>()
            .iter()
        {
            self.draw_map_building_marker(renderer, pos, &BuildingType::TownHall, aspect, panel_w);
        }
        for (pos, _shelter) in self
            .world
            .query::<(&Position, &ecs::ShelterBuilding)>()
            .iter()
        {
            self.draw_map_building_marker(renderer, pos, &BuildingType::Shelter, aspect, panel_w);
        }
        for (pos, proc_bld) in self
            .world
            .query::<(&Position, &ecs::ProcessingBuilding)>()
            .iter()
        {
            let bt = match proc_bld.recipe {
                ecs::Recipe::WoodToPlanks => BuildingType::Workshop,
                ecs::Recipe::StoneToMasonry => BuildingType::Smithy,
                ecs::Recipe::FoodToGrain => BuildingType::Granary,
                ecs::Recipe::GrainToBread => BuildingType::Bakery,
            };
            self.draw_map_building_marker(renderer, pos, &bt, aspect, panel_w);
        }
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
