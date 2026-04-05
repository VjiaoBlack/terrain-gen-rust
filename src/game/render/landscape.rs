use super::{
    CELL_ASPECT, Color, OverlayMode, PANEL_WIDTH, Renderer, Season, Terrain, blend_vegetation,
    landscape_entity_visual,
};
use crate::ecs::{Behavior, BehaviorState, Creature, Position, Species, Sprite};

impl super::super::Game {
    // -----------------------------------------------------------------------
    // Landscape Mode: painterly rendering — texture chars, muted palettes,
    // full Blinn-Phong lighting, seasonal tinting, entities pop via saturation.
    // -----------------------------------------------------------------------

    pub fn draw_landscape_mode(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH;

        // Panel (shared with all modes)
        self.draw_panel(renderer);

        // --- Terrain pass: texture chars + hand-picked palettes + full lighting ---
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
                        renderer.draw(sx, sy, ' ', Color(15, 15, 18), Some(Color(8, 8, 10)));
                        continue;
                    }
                    if let Some(terrain) = self.map.get(wx as usize, wy as usize) {
                        // Check for runtime water depth (pipe_water)
                        let water_depth = self.pipe_water.get_depth(wx as usize, wy as usize);
                        if water_depth > 0.005
                            && !matches!(
                                terrain,
                                Terrain::Water | Terrain::BuildingFloor | Terrain::BuildingWall
                            )
                        {
                            let intensity = (water_depth * 4.0).min(1.0);
                            let water_fg = Color(
                                (30.0 * (1.0 - intensity)) as u8,
                                (60.0 + 40.0 * intensity) as u8,
                                (140.0 + 60.0 * intensity) as u8,
                            );
                            let water_bg = Color(
                                (15.0 * (1.0 - intensity)) as u8,
                                (30.0 + 20.0 * intensity) as u8,
                                (80.0 + 40.0 * intensity) as u8,
                            );
                            let water_chars = ['~', '≈', '∼'];
                            let anim = ((self.tick / 8) as usize + wx as usize + wy as usize) % 3;
                            renderer.draw(sx, sy, water_chars[anim], water_fg, Some(water_bg));
                        } else {
                            let (ch, fg, bg) =
                                self.landscape_terrain_glyph(terrain, wx as usize, wy as usize);
                            let (ch, fg, bg) =
                                self.worn_terrain_override(wx as usize, wy as usize, ch, fg, bg);
                            renderer.draw(sx, sy, ch, fg, Some(bg));
                        }
                    }
                }
            }
        }

        // Building center glyphs: show building-type icon on center tile
        // Landscape mode has no day/night lighting applied to terrain, so skip lighting.
        self.draw_building_center_overlays(renderer, false);

        // --- Entity pass: saturated colors pop against muted terrain ---
        for (e, (pos, sprite)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Sprite))>()
            .iter()
        {
            let creature_opt = self.world.get::<&Creature>(e).ok();
            let bstate = self.world.get::<&Behavior>(e).ok().map(|b| b.state);

            // Hide AtHome entities
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

            let (display_ch, display_fg) =
                if let (Some(creature), Some(bstate_val)) = (creature_opt.as_deref(), &bstate) {
                    match landscape_entity_visual(creature.species, bstate_val) {
                        Some(vis) => vis,
                        None => continue,
                    }
                } else {
                    // Non-creature entities keep their sprite
                    (sprite.ch, sprite.fg)
                };

            let sx_i = (pos.x.round() as i32 - self.camera.x) * aspect + panel_w as i32;
            let sy_i = pos.y.round() as i32 - self.camera.y;
            if sx_i >= panel_w as i32
                && sy_i >= 0
                && (sx_i as u16) < w
                && (sy_i as u16) < h.saturating_sub(status_h)
            {
                // Entities render at full saturation — no lighting dimming
                // Sleeping entities get slight dimming.
                // Hauler brightness: seeking/idle villagers at 70%, haulers at 100%
                let dim = if matches!(bstate, Some(BehaviorState::Sleeping { .. })) {
                    0.6
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
                    (display_fg.0 as f64 * dim).clamp(0.0, 255.0) as u8,
                    (display_fg.1 as f64 * dim).clamp(0.0, 255.0) as u8,
                    (display_fg.2 as f64 * dim).clamp(0.0, 255.0) as u8,
                );
                // Transparent bg: entity floats on landscape
                renderer.draw(sx_i as u16, sy_i as u16, display_ch, fg, None);
            }
        }

        // --- Shared UI overlays ---
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
        self.draw_weather(renderer);
        self.draw_minimap(renderer);
        self.draw_status(renderer);
    }

    /// Resolve terrain glyph for Landscape Mode: texture char + season-tinted
    /// palette + full Blinn-Phong lighting.
    fn landscape_terrain_glyph(
        &self,
        terrain: &Terrain,
        wx: usize,
        wy: usize,
    ) -> (char, Color, Color) {
        // Base texture character driven by vegetation density
        let veg = self.vegetation.get(wx, wy);
        let mut ch = terrain.landscape_ch(wx, wy, veg);

        // Blend soil base with landscape vegetation color based on vegetation density.
        // For vegetation-sensitive terrains, landscape_fg/bg serve as the "fully
        // vegetated" target and soil_fg/bg as the bare-ground base.
        // Soil color from actual SoilType grid, not biome enum
        let idx = wy * self.map.width + wx;
        let soil = if idx < self.soil.len() {
            self.soil[idx]
        } else {
            crate::terrain_pipeline::SoilType::Loam
        };
        let mut fg = if terrain.has_vegetation_blending() {
            let temp = if idx < self.pipeline_temperature.len() {
                self.pipeline_temperature[idx]
            } else {
                0.5
            };
            let moist = self.moisture.get(wx, wy);
            let vc = crate::tilemap::vegetation_color_from_conditions(moist, temp);
            blend_vegetation(soil.ground_fg(), vc, veg)
        } else {
            terrain.landscape_fg()
        };
        let mut bg = if terrain.has_vegetation_blending() {
            let temp = if idx < self.pipeline_temperature.len() {
                self.pipeline_temperature[idx]
            } else {
                0.5
            };
            let moist = self.moisture.get(wx, wy);
            let vc = crate::tilemap::vegetation_color_from_conditions(moist, temp);
            let vc_dark = Color(
                vc.0.saturating_sub(25),
                vc.1.saturating_sub(25),
                vc.2.saturating_sub(5),
            );
            blend_vegetation(soil.ground_bg(), vc_dark, veg)
        } else {
            terrain.landscape_bg()
        };

        // Vegetation overlay: dense vegetation overrides base texture chars
        if matches!(
            terrain,
            Terrain::Grass | Terrain::Scrubland | Terrain::Bare | Terrain::Sapling
        ) {
            if wx < self.vegetation.width && wy < self.vegetation.height {
                let v = self.vegetation.get(wx, wy);
                if v > 0.8 {
                    // Dense canopy
                    let pool: &[char] = &['%', '#', '&', '@'];
                    let idx = (wx.wrapping_mul(7).wrapping_add(wy.wrapping_mul(13))) % pool.len();
                    ch = pool[idx];
                } else if v > 0.5 {
                    // Brush, young trees
                    let pool: &[char] = &['%', ':', '"', ';'];
                    let idx = (wx.wrapping_mul(7).wrapping_add(wy.wrapping_mul(13))) % pool.len();
                    ch = pool[idx];
                } else if v > 0.2 {
                    // Light scrub
                    let pool: &[char] = &['"', ',', '\'', ';'];
                    let idx = (wx.wrapping_mul(7).wrapping_add(wy.wrapping_mul(13))) % pool.len();
                    ch = pool[idx];
                }
            }
        }

        // River rendering from discharge field (Nick McDonald's approach):
        // Skip on Terrain::Water — ocean already has its own rendering.
        // Credit: https://github.com/weigert/SimpleHydrology
        let idx = wy * self.map.width + wx;
        let river_alpha = if *terrain == Terrain::Water {
            0.0 // ocean handles its own rendering
        } else if idx < self.discharge.len() {
            crate::hydrology::erf_approx(0.4 * self.discharge[idx])
        } else {
            0.0
        };
        if river_alpha > 0.1 {
            let alpha = river_alpha.min(0.9);
            // Nick's waterColor = (92, 133, 142)
            fg = Color(
                (fg.0 as f64 * (1.0 - alpha) + 92.0 * alpha) as u8,
                (fg.1 as f64 * (1.0 - alpha) + 133.0 * alpha) as u8,
                (fg.2 as f64 * (1.0 - alpha) + 142.0 * alpha) as u8,
            );
            bg = Color(
                (bg.0 as f64 * (1.0 - alpha) + 60.0 * alpha) as u8,
                (bg.1 as f64 * (1.0 - alpha) + 90.0 * alpha) as u8,
                (bg.2 as f64 * (1.0 - alpha) + 100.0 * alpha) as u8,
            );
            if alpha > 0.5 {
                ch = '~';
            } else if alpha > 0.2 {
                ch = '·';
            }
        }

        // Apply seasonal palette shift
        fg = self.landscape_season_tint(fg, terrain);
        bg = self.landscape_season_tint(bg, terrain);

        // Apply full Blinn-Phong lighting (this is where the stepped lighting shines)
        fg = self.day_night.apply_lighting(fg, wx, wy);
        bg = self.day_night.apply_lighting(bg, wx, wy);

        (ch, fg, bg)
    }

    /// Seasonal color tinting for Landscape Mode.
    /// Uses the hand-picked palette shifts from the design doc.
    fn landscape_season_tint(&self, color: Color, terrain: &Terrain) -> Color {
        let Color(r, g, b) = color;
        match self.day_night.season {
            Season::Spring => match terrain {
                Terrain::Grass | Terrain::Bare | Terrain::Sapling => Color(
                    r,
                    (g as u16 + 15).min(255) as u8,
                    (b as u16 + 5).min(255) as u8,
                ),
                Terrain::Forest => Color(
                    r,
                    (g as u16 + 12).min(255) as u8,
                    (b as u16 + 5).min(255) as u8,
                ),
                Terrain::Scrubland => Color(
                    (r as u16 + 5).min(255) as u8,
                    (g as u16 + 10).min(255) as u8,
                    b,
                ),
                Terrain::Marsh => Color(
                    (r as u16 + 5).min(255) as u8,
                    (g as u16 + 15).min(255) as u8,
                    (b as u16 + 10).min(255) as u8,
                ),
                Terrain::Snow => Color(r, g, (b as i16 - 15).max(0) as u8),
                _ => color,
            },
            Season::Summer => match terrain {
                Terrain::Grass | Terrain::Bare | Terrain::Sapling => Color(
                    (r as u16 + 10).min(255) as u8,
                    (g as u16 + 5).min(255) as u8,
                    (b as i16 - 10).max(0) as u8,
                ),
                Terrain::Forest => Color(
                    (r as u16 + 5).min(255) as u8,
                    (g as u16 + 8).min(255) as u8,
                    (b as i16 - 5).max(0) as u8,
                ),
                Terrain::Desert => Color(
                    (r as u16 + 15).min(255) as u8,
                    (g as u16 + 5).min(255) as u8,
                    (b as i16 - 10).max(0) as u8,
                ),
                Terrain::Sand => Color(
                    (r as u16 + 10).min(255) as u8,
                    (g as u16 + 5).min(255) as u8,
                    (b as i16 - 5).max(0) as u8,
                ),
                Terrain::Tundra => Color(
                    (r as u16 + 10).min(255) as u8,
                    (g as u16 + 5).min(255) as u8,
                    (b as i16 - 5).max(0) as u8,
                ),
                _ => color,
            },
            Season::Autumn => match terrain {
                Terrain::Grass | Terrain::Bare | Terrain::Sapling => {
                    // Golden brown
                    Color(110, 90, 35)
                }
                Terrain::Forest => {
                    // Deep orange-red
                    Color(100, 55, 18)
                }
                Terrain::Scrubland => {
                    // Russet
                    Color(120, 85, 30)
                }
                Terrain::Marsh => Color(
                    (r as u16 + 15).min(255) as u8,
                    (g as i16 - 10).max(0) as u8,
                    (b as i16 - 10).max(0) as u8,
                ),
                Terrain::Mountain => Color(
                    (r as u16 + 5).min(255) as u8,
                    g,
                    (b as i16 - 5).max(0) as u8,
                ),
                _ => color,
            },
            Season::Winter => match terrain {
                Terrain::Grass | Terrain::Bare | Terrain::Sapling => {
                    // Frost/snow dusted
                    Color(140, 145, 155)
                }
                Terrain::Forest => {
                    // Bare, dark, cold
                    Color(50, 60, 55)
                }
                Terrain::Scrubland => {
                    // Dead scrub
                    Color(110, 108, 100)
                }
                Terrain::Marsh => {
                    // Frozen grey
                    Color(55, 65, 70)
                }
                Terrain::Sand => Color(
                    (r as u16 + 20).min(255) as u8,
                    (g as u16 + 20).min(255) as u8,
                    (b as u16 + 30).min(255) as u8,
                ),
                Terrain::Tundra => {
                    // Heavy snow
                    Color(195, 198, 210)
                }
                Terrain::Snow => {
                    // Fresh powder
                    Color(225, 225, 240)
                }
                Terrain::Mountain => Color(
                    (r as u16 + 15).min(255) as u8,
                    (g as u16 + 15).min(255) as u8,
                    (b as u16 + 25).min(255) as u8,
                ),
                _ => color,
            },
        }
    }
}
