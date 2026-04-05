use super::{
    CELL_ASPECT, Color, OverlayMode, PANEL_WIDTH, Renderer, Terrain, Velocity, blend_vegetation,
    entity_visual,
};
use crate::ecs::{Behavior, BehaviorState, Creature, Position, Species, Sprite};

impl super::super::Game {
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
                    // Dirty-rect: skip clean tiles (terminal double-buffer retains previous)
                    if !self.dirty.is_dirty(wx as usize, wy as usize) {
                        continue;
                    }
                    // Fog of exploration: unrevealed tiles render as dark fog
                    if !self.exploration.is_revealed(wx as usize, wy as usize) {
                        renderer.draw(sx, sy, '░', Color(30, 30, 30), Some(Color(10, 10, 10)));
                        continue;
                    }
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        if *terrain == Terrain::Water {
                            // Water terrain: animated character + blue shimmer
                            let water_chars = ['~', '≈', '∼'];
                            let anim_index =
                                ((self.tick / 8) as usize + wx as usize + wy as usize) % 3;
                            let ch = water_chars[anim_index];

                            let Color(r, g, b) = terrain.fg();
                            let shimmer =
                                (((self.tick as f64) * 0.1 + (wx as f64)).sin() * 20.0) as i16;
                            let b_shimmered = (b as i16 + shimmer).clamp(0, 255) as u8;
                            let fg = Color(r, g, b_shimmered);

                            let bg = terrain.bg().map(|Color(br, bg_g, bb)| {
                                let bb_shimmered = (bb as i16 + shimmer).clamp(0, 255) as u8;
                                Color(br, bg_g, bb_shimmered)
                            });

                            renderer.draw(sx, sy, ch, fg, bg);
                        } else {
                            // Check for runtime water depth — render as water if flooded
                            // Use pipe_water for responsive physics-based depth
                            let water_depth = self.pipe_water.get_depth(wx as usize, wy as usize);
                            if water_depth > 0.005
                                && !matches!(
                                    terrain,
                                    Terrain::Water | Terrain::BuildingFloor | Terrain::BuildingWall
                                )
                            {
                                let intensity = (water_depth * 4.0).min(1.0);
                                let water_fg = Color(
                                    (40.0 * (1.0 - intensity)) as u8,
                                    (80.0 + 60.0 * intensity) as u8,
                                    (160.0 + 60.0 * intensity) as u8,
                                );
                                let water_bg = Color(
                                    (20.0 * (1.0 - intensity)) as u8,
                                    (40.0 + 30.0 * intensity) as u8,
                                    (100.0 + 40.0 * intensity) as u8,
                                );
                                let water_chars = ['~', '≈', '∼'];
                                let anim =
                                    ((self.tick / 8) as usize + wx as usize + wy as usize) % 3;
                                let fg = self.day_night.apply_lighting(
                                    water_fg,
                                    wx as usize,
                                    wy as usize,
                                );
                                let bg = Some(self.day_night.apply_lighting(
                                    water_bg,
                                    wx as usize,
                                    wy as usize,
                                ));
                                renderer.draw(sx, sy, water_chars[anim], fg, bg);
                                continue;
                            }

                            // River rendering from discharge field (Nick McDonald's approach):
                            // Blend terrain toward water color based on erf(0.4 * discharge).
                            let ux = wx as usize;
                            let uy = wy as usize;
                            let idx = uy * self.map.width + ux;
                            let river_alpha = if idx < self.discharge.len() {
                                crate::hydrology::erf_approx(0.4 * self.discharge[idx])
                            } else {
                                0.0
                            };
                            if river_alpha > 0.1 {
                                // River tile: blend toward blue-gray water color
                                let alpha = river_alpha.min(0.9);
                                let water_r = 92.0;
                                let water_g = 133.0;
                                let water_b = 142.0;
                                let base_fg = terrain.fg();
                                let r = (base_fg.0 as f64 * (1.0 - alpha) + water_r * alpha) as u8;
                                let g = (base_fg.1 as f64 * (1.0 - alpha) + water_g * alpha) as u8;
                                let b = (base_fg.2 as f64 * (1.0 - alpha) + water_b * alpha) as u8;
                                let fg = self.day_night.apply_lighting(
                                    Color(r, g, b), ux, uy,
                                );
                                let bg_r = (r as f64 * 0.6) as u8;
                                let bg_g = (g as f64 * 0.6) as u8;
                                let bg_b = (b as f64 * 0.6) as u8;
                                let bg = Some(self.day_night.apply_lighting(
                                    Color(bg_r, bg_g, bg_b), ux, uy,
                                ));
                                let ch = if alpha > 0.5 { '~' } else { '·' };
                                renderer.draw(sx, sy, ch, fg, bg);
                                continue;
                            }

                            // Blend soil + vegetation for natural terrain types
                            let (fg, bg) = if terrain.has_vegetation_blending() {
                                let ux = wx as usize;
                                let uy = wy as usize;
                                let idx = uy * self.map.width + ux;
                                let veg = self.vegetation.get(ux, uy);
                                // Soil color from actual SoilType, not biome enum
                                let soil = if idx < self.soil.len() {
                                    self.soil[idx]
                                } else {
                                    crate::terrain_pipeline::SoilType::Loam
                                };
                                // Vegetation color from conditions, not frozen biome
                                let temp = if idx < self.pipeline_temperature.len() {
                                    self.pipeline_temperature[idx]
                                } else {
                                    0.5
                                };
                                let moist = self.moisture.get(ux, uy);
                                let vc =
                                    crate::tilemap::vegetation_color_from_conditions(moist, temp);
                                let vc_dark = Color(
                                    vc.0.saturating_sub(25),
                                    vc.1.saturating_sub(25),
                                    vc.2.saturating_sub(5),
                                );
                                let fg = blend_vegetation(soil.ground_fg(), vc, veg);
                                let bg_color = blend_vegetation(soil.ground_bg(), vc_dark, veg);
                                (
                                    self.season_tint(fg, terrain),
                                    Some(self.season_tint(bg_color, terrain)),
                                )
                            } else {
                                (
                                    self.season_tint(terrain.fg(), terrain),
                                    terrain.bg().map(|c| self.season_tint(c, terrain)),
                                )
                            };
                            // Apply worn terrain visual from foot traffic
                            let base_bg = bg.unwrap_or(Color(0, 0, 0));
                            let (ch, fg, worn_bg) = self.worn_terrain_override(
                                wx as usize,
                                wy as usize,
                                terrain.ch(),
                                fg,
                                base_bg,
                            );
                            let fg = self.day_night.apply_lighting(fg, wx as usize, wy as usize);
                            let bg = self.day_night.apply_lighting_bg(
                                Some(worn_bg),
                                wx as usize,
                                wy as usize,
                            );
                            renderer.draw(sx, sy, ch, fg, bg);
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
                if wx >= 0
                    && wy >= 0
                    && (wx as usize) < self.vegetation.width
                    && (wy as usize) < self.vegetation.height
                {
                    if !self.dirty.is_dirty(wx as usize, wy as usize) {
                        continue;
                    }
                    if !self.exploration.is_revealed(wx as usize, wy as usize) {
                        continue;
                    }
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
                        // Background: use blended soil+vegetation color
                        let ux = wx as usize;
                        let uy = wy as usize;
                        let bg = self.map.get(ux, uy).map(|t| {
                            if t.has_vegetation_blending() {
                                let idx = uy * self.map.width + ux;
                                let soil = if idx < self.soil.len() {
                                    self.soil[idx]
                                } else {
                                    crate::terrain_pipeline::SoilType::Loam
                                };
                                let temp_v = if idx < self.pipeline_temperature.len() {
                                    self.pipeline_temperature[idx]
                                } else {
                                    0.5
                                };
                                let moist_v = self.moisture.get(ux, uy);
                                let vc = crate::tilemap::vegetation_color_from_conditions(
                                    moist_v, temp_v,
                                );
                                let vc_dark = Color(
                                    vc.0.saturating_sub(25),
                                    vc.1.saturating_sub(25),
                                    vc.2.saturating_sub(5),
                                );
                                let bg_color = blend_vegetation(soil.ground_bg(), vc_dark, v);
                                self.day_night.apply_lighting(bg_color, ux, uy)
                            } else {
                                let c = t.bg().unwrap_or(Color(0, 0, 0));
                                self.day_night.apply_lighting(c, ux, uy)
                            }
                        });
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
                if wx >= 0
                    && wy >= 0
                    && (wx as usize) < self.water.width
                    && (wy as usize) < self.water.height
                {
                    if !self.dirty.is_dirty(wx as usize, wy as usize) {
                        continue;
                    }
                    if !self.exploration.is_revealed(wx as usize, wy as usize) {
                        continue;
                    }
                    // Skip ocean tiles — they already have their own water appearance
                    if matches!(self.map.get(wx as usize, wy as usize), Some(Terrain::Water)) {
                        continue;
                    }
                    let depth = self.pipe_water.get_depth(wx as usize, wy as usize);
                    if depth > 0.05 {
                        let intensity = (depth * 5.0).min(1.0);
                        let r = (50.0 * (1.0 - intensity)) as u8;
                        let g = (100.0 + 50.0 * intensity) as u8;
                        let b_base = (180.0 + 75.0 * intensity) as u8;

                        // Animated character cycling
                        let water_chars = ['~', '≈', '∼'];
                        let anim_index = ((self.tick / 8) as usize + wx as usize + wy as usize) % 3;
                        let ch = water_chars[anim_index];

                        // Blue channel shimmer
                        let shimmer =
                            (((self.tick as f64) * 0.1 + (wx as f64)).sin() * 20.0) as i16;
                        let b = (b_base as i16 + shimmer).clamp(0, 255) as u8;

                        let fg =
                            self.day_night
                                .apply_lighting(Color(r, g, b), wx as usize, wy as usize);
                        let bg_b = ((80.0 + 40.0 * intensity) as i16 + shimmer).clamp(0, 255) as u8;
                        let bg = self.day_night.apply_lighting_bg(
                            Some(Color(20, 40, bg_b)),
                            wx as usize,
                            wy as usize,
                        );
                        renderer.draw(sx, sy, ch, fg, bg);
                    }
                }
            }
        }

        // Territory tint: only shown in Territory overlay mode
        if self.overlay == OverlayMode::Territory {
            for sy in 0..h.saturating_sub(status_h) {
                for sx in panel_w..w {
                    let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                    let wy = self.camera.y + sy as i32;
                    if wx >= 0
                        && wy >= 0
                        && (wx as usize) < self.influence.width
                        && (wy as usize) < self.influence.height
                    {
                        if !self.dirty.is_dirty(wx as usize, wy as usize) {
                            continue;
                        }
                        if !self.exploration.is_revealed(wx as usize, wy as usize) {
                            continue;
                        }
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
        } // end Territory overlay

        // Building center glyphs: show building-type icon on center tile
        self.draw_building_center_overlays(renderer, true);

        // draw entities (offset by camera) — world→screen X is multiplied by aspect
        // Skip AtHome (hidden in den for prey), use entity_visual() for state-based rendering.
        for (e, (pos, sprite)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Sprite))>()
            .iter()
        {
            let bstate = self.world.get::<&Behavior>(e).ok().map(|b| b.state);
            if matches!(bstate, Some(BehaviorState::AtHome { .. })) {
                continue;
            }
            // Hide entities on unexplored tiles
            if !self
                .exploration
                .is_revealed(pos.x.round() as usize, pos.y.round() as usize)
            {
                continue;
            }
            let sx = (pos.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
            let sy = pos.y.round() as i32 - self.camera.y;
            if sx >= panel_w as i32
                && sy >= 0
                && (sx as u16) < w
                && (sy as u16) < h.saturating_sub(status_h)
            {
                let (tr, tg, tb) = self.day_night.ambient_tint();

                // Determine display char and base color from species + behavior state.
                // Non-creature entities (buildings, resources) keep their sprite defaults.
                let creature_opt = self.world.get::<&Creature>(e).ok();
                let vel_opt = self.world.get::<&Velocity>(e).ok();
                let (display_ch, base_fg) = if let (Some(creature), Some(bstate_val)) =
                    (creature_opt.as_deref(), &bstate)
                {
                    let (vdx, vdy) = vel_opt
                        .as_deref()
                        .map(|v| (v.dx, v.dy))
                        .unwrap_or((0.0, 0.0));
                    entity_visual(creature.species, bstate_val, vdx, vdy, sprite)
                } else {
                    (sprite.ch, sprite.fg)
                };

                // Apply day/night tinting; sleeping entities get extra dimming.
                // Hauler brightness: hauling villagers render at full brightness,
                // seeking/idle villagers render at 70% to create visible directional flow.
                let dim = if matches!(bstate, Some(BehaviorState::Sleeping { .. })) {
                    0.5
                } else if matches!(
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
                    (base_fg.0 as f64 * tr * dim).clamp(0.0, 255.0) as u8,
                    (base_fg.1 as f64 * tg * dim).clamp(0.0, 255.0) as u8,
                    (base_fg.2 as f64 * tb * dim).clamp(0.0, 255.0) as u8,
                );
                renderer.draw(sx as u16, sy as u16, display_ch, fg, None);
            }
        }

        // Draw particles (smoke, effects) on top of entities
        for p in &self.particles {
            let sx = (p.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
            let sy = p.y.round() as i32 - self.camera.y;
            if sx >= panel_w as i32
                && sy >= 0
                && (sx as u16) < w
                && (sy as u16) < h.saturating_sub(status_h)
            {
                // Apply color fade in final 40% of particle lifetime
                let fg = if p.max_life > 0 {
                    let age_fraction = 1.0 - (p.life as f64 / p.max_life as f64);
                    if age_fraction > 0.6 {
                        let fade = 1.0 - ((age_fraction - 0.6) / 0.4);
                        Color(
                            (p.fg.0 as f64 * fade) as u8,
                            (p.fg.1 as f64 * fade) as u8,
                            (p.fg.2 as f64 * fade) as u8,
                        )
                    } else {
                        p.fg
                    }
                } else {
                    p.fg
                };
                renderer.draw(sx as u16, sy as u16, p.ch, fg, None);
            }
        }

        // Overlay pass: draw additional markers on top
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
        self.draw_weather(renderer);
        self.draw_minimap(renderer);
        self.draw_status(renderer);
    }
}
