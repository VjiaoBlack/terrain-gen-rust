use super::{CELL_ASPECT, GameEvent, OverlayMode, PANEL_WIDTH, ROAD_TRAFFIC_THRESHOLD};
use crate::ecs::{
    self, Behavior, BehaviorState, BuildingType, Creature, Den, FarmPlot, FoodSource,
    GarrisonBuilding, Position, ProcessingBuilding, ResourceType, Species, Sprite, Stockpile,
    StoneDeposit,
};
use crate::renderer::{Color, Renderer};
use crate::simulation::Season;
use crate::tilemap::Terrain;

impl super::Game {
    /// Apply seasonal color tinting to vegetation-sensitive terrain.
    fn season_tint(&self, color: Color, terrain: &Terrain) -> Color {
        use crate::simulation::Season;
        match terrain {
            Terrain::Grass | Terrain::Forest => {
                let Color(r, g, b) = color;
                match self.day_night.season {
                    Season::Spring => {
                        // Fresh greens, slightly brighter
                        Color(r, (g as u16 + 15).min(255) as u8, b)
                    }
                    Season::Summer => {
                        // Warm, lush — boost green slightly, add warmth
                        Color(
                            (r as u16 + 10).min(255) as u8,
                            (g as u16 + 10).min(255) as u8,
                            b,
                        )
                    }
                    Season::Autumn => {
                        // Strong orange/brown/red shift
                        let r2 = (r as u16 + 80).min(255) as u8;
                        let g2 = (g as i16 - 30).max(0) as u8;
                        let b2 = (b as i16 - 10).max(0) as u8;
                        Color(r2, g2, b2)
                    }
                    Season::Winter => {
                        // Heavy frost: desaturate toward white/gray
                        let avg = (r as u16 + g as u16 + b as u16) / 3;
                        let frost = |c: u8| ((c as u16 + avg * 2) / 3).min(255) as u8;
                        Color(frost(r), frost(g), (frost(b) as u16 + 20).min(255) as u8)
                    }
                }
            }
            Terrain::Sand => {
                match self.day_night.season {
                    Season::Winter => {
                        // Snow-dusted sand
                        let Color(r, g, b) = color;
                        Color(
                            (r as u16 + 30).min(255) as u8,
                            (g as u16 + 30).min(255) as u8,
                            (b as u16 + 40).min(255) as u8,
                        )
                    }
                    _ => color,
                }
            }
            _ => color,
        }
    }

    /// Draw the left-side UI panel.
    fn draw_panel(&self, renderer: &mut dyn Renderer) {
        let (_w, h) = renderer.size();
        let pw = PANEL_WIDTH as usize;
        let bg = Color(25, 25, 40);
        let fg = Color(200, 200, 200);
        let dim = Color(120, 120, 140);
        let highlight = Color(255, 220, 100);
        let green = Color(100, 220, 100);

        // Helper: draw a line of text in the panel
        let mut row = 0u16;
        let draw_line = |r: &mut dyn Renderer, y: u16, text: &str, color: Color| {
            for (i, ch) in text.chars().enumerate() {
                if i < pw {
                    r.draw(i as u16, y, ch, color, Some(bg));
                }
            }
            for i in text.len()..pw {
                r.draw(i as u16, y, ' ', fg, Some(bg));
            }
        };

        // Fill panel background
        for y in 0..h {
            for x in 0..PANEL_WIDTH {
                renderer.draw(x, y, ' ', fg, Some(bg));
            }
        }

        // Header
        draw_line(renderer, row, " TERRAIN-GEN", highlight);
        row += 1;

        // Separator
        let sep: String = std::iter::repeat_n('-', pw).collect();
        draw_line(renderer, row, &sep, dim);
        row += 1;

        // Season + Date
        let season_icon = match self.day_night.season {
            Season::Spring => "🌱",
            Season::Summer => "☀",
            Season::Autumn => "🍂",
            Season::Winter => "❄",
        };
        draw_line(
            renderer,
            row,
            &format!(" {} {}", season_icon, self.day_night.season.name()),
            highlight,
        );
        row += 1;
        let date = self.day_night.date_string();
        let time = self.day_night.time_string();
        draw_line(renderer, row, &format!(" {} {}", date, time), fg);
        row += 1;

        // Temperature feel based on season
        let temp = match self.day_night.season {
            Season::Spring => "Mild",
            Season::Summer => "Warm",
            Season::Autumn => "Cool",
            Season::Winter => "Freezing",
        };
        let night_str = if self.day_night.is_night() {
            " (night)"
        } else {
            ""
        };
        let speed_str = if self.game_speed > 1 {
            format!("  [{}x]", self.game_speed)
        } else {
            String::new()
        };
        draw_line(
            renderer,
            row,
            &format!(" {}{}{}", temp, night_str, speed_str),
            dim,
        );
        row += 1;
        row += 1;

        // Population
        let villager_count = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        let wolf_count = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        draw_line(
            renderer,
            row,
            &format!(" Pop: {}  Wolves: {}", villager_count, wolf_count),
            fg,
        );
        row += 1;
        row += 1;

        // Resources
        draw_line(renderer, row, " Resources", highlight);
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Food:  {}", self.resources.food),
            fg,
        );
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Wood:  {}", self.resources.wood),
            fg,
        );
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Stone: {}", self.resources.stone),
            fg,
        );
        row += 1;
        if self.resources.planks > 0
            || self.resources.masonry > 0
            || self.resources.grain > 0
            || self.resources.bread > 0
        {
            draw_line(
                renderer,
                row,
                &format!("  Planks:  {}", self.resources.planks),
                dim,
            );
            row += 1;
            draw_line(
                renderer,
                row,
                &format!("  Masonry: {}", self.resources.masonry),
                dim,
            );
            row += 1;
            draw_line(
                renderer,
                row,
                &format!("  Grain:   {}", self.resources.grain),
                dim,
            );
            row += 1;
            if self.resources.bread > 0 {
                draw_line(
                    renderer,
                    row,
                    &format!("  Bread:   {}", self.resources.bread),
                    dim,
                );
                row += 1;
            }
        }
        row += 1;

        // Population
        let villagers = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();
        let prey = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Prey)
            .count();
        let wolves = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        draw_line(renderer, row, " Population", highlight);
        row += 1;
        draw_line(renderer, row, &format!("  Villagers: {}", villagers), fg);
        row += 1;
        draw_line(renderer, row, &format!("  Rabbits:   {}", prey), dim);
        row += 1;
        draw_line(renderer, row, &format!("  Wolves:    {}", wolves), dim);
        row += 1;
        row += 1;

        // Skills section
        let skill_color = Color(180, 160, 220);
        draw_line(renderer, row, " Skills", highlight);
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Farm  {:4.1}", self.skills.farming),
            skill_color,
        );
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Mine  {:4.1}", self.skills.mining),
            skill_color,
        );
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Wood  {:4.1}", self.skills.woodcutting),
            skill_color,
        );
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Build {:4.1}", self.skills.building),
            skill_color,
        );
        row += 1;
        draw_line(
            renderer,
            row,
            &format!("  Milit {:4.1}", self.skills.military),
            skill_color,
        );
        row += 1;
        row += 1;

        // Build section
        draw_line(renderer, row, " Build (click/[b])", highlight);
        row += 1;
        let types = BuildingType::all();
        for bt in types {
            let c = bt.cost();
            let selected = self.build_mode && self.selected_building == *bt;
            let marker = if selected { ">" } else { " " };
            let mut cost_parts: Vec<String> = Vec::new();
            if c.food > 0 {
                cost_parts.push(format!("f:{}", c.food));
            }
            if c.wood > 0 {
                cost_parts.push(format!("w:{}", c.wood));
            }
            if c.stone > 0 {
                cost_parts.push(format!("s:{}", c.stone));
            }
            if c.planks > 0 {
                cost_parts.push(format!("P:{}", c.planks));
            }
            if c.masonry > 0 {
                cost_parts.push(format!("M:{}", c.masonry));
            }
            let line = format!("{} {} {}", marker, bt.name(), cost_parts.join(" "));
            let color = if selected { green } else { fg };
            draw_line(renderer, row, &line, color);
            row += 1;
        }
        row += 1;

        // Auto-build toggle
        let ab_str = if self.auto_build { "ON" } else { "off" };
        draw_line(
            renderer,
            row,
            &format!(" Auto-build [a]: {}", ab_str),
            if self.auto_build { green } else { fg },
        );
        row += 1;
        row += 1;

        // Overlay toggle
        let ov_str = match self.overlay {
            OverlayMode::None => "off",
            OverlayMode::Tasks => "TASKS",
            OverlayMode::Resources => "RESOURCES",
            OverlayMode::Threats => "THREATS",
            OverlayMode::Traffic => "TRAFFIC",
            OverlayMode::Territory => "TERRITORY",
            OverlayMode::Elevation => "ELEVATION",
        };
        draw_line(
            renderer,
            row,
            &format!(" Overlay [o]: {}", ov_str),
            if self.overlay != OverlayMode::None {
                green
            } else {
                fg
            },
        );
        row += 1;

        // Active events
        if !self.events.active_events.is_empty() {
            row += 1;
            draw_line(renderer, row, " Events", Color(255, 200, 50));
            row += 1;
            for event in &self.events.active_events {
                let (name, remaining) = match event {
                    GameEvent::Drought { ticks_remaining } => ("Drought", *ticks_remaining),
                    GameEvent::BountifulHarvest { ticks_remaining } => {
                        ("Harvest+", *ticks_remaining)
                    }
                    GameEvent::WolfSurge { ticks_remaining } => ("Wolf Surge", *ticks_remaining),
                    GameEvent::Plague {
                        ticks_remaining, ..
                    } => ("Plague", *ticks_remaining),
                    GameEvent::Blizzard { ticks_remaining } => ("Blizzard", *ticks_remaining),
                    GameEvent::Migration { count } => {
                        draw_line(renderer, row, &format!("  +{} migrants", count), green);
                        row += 1;
                        continue;
                    }
                    GameEvent::BanditRaid { .. } => {
                        draw_line(renderer, row, "  Bandit Raid!", Color(200, 50, 50));
                        row += 1;
                        continue;
                    }
                };
                let color = match event {
                    GameEvent::Drought { .. } => Color(200, 100, 50),
                    GameEvent::BountifulHarvest { .. } => Color(50, 200, 50),
                    GameEvent::WolfSurge { .. } => Color(200, 50, 50),
                    GameEvent::Plague { .. } => Color(180, 50, 180),
                    GameEvent::Blizzard { .. } => Color(150, 200, 255),
                    _ => fg,
                };
                draw_line(
                    renderer,
                    row,
                    &format!("  {} ({}t)", name, remaining),
                    color,
                );
                row += 1;
            }
        }
        row += 1;

        // Controls
        draw_line(renderer, row, " Controls", highlight);
        row += 1;
        draw_line(renderer, row, "  arrows: scroll", dim);
        row += 1;
        draw_line(renderer, row, "  [b] build  [k] query", dim);
        row += 1;
        draw_line(renderer, row, "  [o] overlay [f] speed", dim);
        row += 1;
        draw_line(renderer, row, "  [g] goto  [a] auto", dim);
        row += 1;
        draw_line(renderer, row, "  [space] pause [q] quit", dim);
        row += 1;
        if self.build_mode {
            draw_line(renderer, row, "  wasd:move tab:type", dim);
            row += 1;
            draw_line(renderer, row, "  enter:place [x] demo", dim);
            row += 1;
        }

        // Mode indicator
        if row + 2 < h {
            row += 1;
            if self.build_mode {
                draw_line(renderer, row, " MODE: BUILD", green);
            } else if self.query_mode {
                draw_line(renderer, row, " MODE: QUERY", Color(200, 100, 255));
            } else if self.paused {
                draw_line(renderer, row, " PAUSED", Color(255, 100, 100));
            }
        }
    }

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
                            let fg = self.season_tint(terrain.fg(), terrain);
                            let bg = terrain.bg().map(|c| self.season_tint(c, terrain));
                            let fg = self.day_night.apply_lighting(fg, wx as usize, wy as usize);
                            let bg = self
                                .day_night
                                .apply_lighting_bg(bg, wx as usize, wy as usize);
                            renderer.draw(sx, sy, terrain.ch(), fg, bg);
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
                        // Keep terrain bg underneath vegetation
                        let bg = self
                            .map
                            .get(wx as usize, wy as usize)
                            .and_then(|t| t.bg())
                            .map(|c| self.day_night.apply_lighting(c, wx as usize, wy as usize));
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
                    if !self.exploration.is_revealed(wx as usize, wy as usize) {
                        continue;
                    }
                    // Skip ocean tiles — they already have their own water appearance
                    if matches!(self.map.get(wx as usize, wy as usize), Some(Terrain::Water)) {
                        continue;
                    }
                    let depth = self.water.get_avg(wx as usize, wy as usize);
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

        // draw entities (offset by camera) — world→screen X is multiplied by aspect
        // Skip AtHome (hidden in den), dim Captured (being eaten)
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
                // Task overlay: color-code villagers by activity
                let fg = if self.overlay == OverlayMode::Tasks {
                    if let Ok(creature) = self.world.get::<&Creature>(e) {
                        if creature.species == Species::Villager {
                            match bstate {
                                Some(BehaviorState::Gathering {
                                    resource_type: ResourceType::Wood,
                                    ..
                                }) => Color(139, 90, 43), // brown
                                Some(BehaviorState::Gathering {
                                    resource_type: ResourceType::Stone,
                                    ..
                                }) => Color(150, 150, 150), // gray
                                Some(BehaviorState::Gathering {
                                    resource_type: ResourceType::Food,
                                    ..
                                }) => Color(50, 200, 50), // green
                                Some(BehaviorState::Hauling { .. }) => Color(200, 180, 50), // gold
                                Some(BehaviorState::Building { .. }) => Color(255, 220, 50), // yellow
                                Some(BehaviorState::Farming { .. }) => Color(80, 200, 80), // farm green
                                Some(BehaviorState::Working { .. }) => Color(200, 120, 50), // workshop orange
                                Some(BehaviorState::Exploring { .. }) => Color(150, 50, 255), // purple - exploring
                                Some(BehaviorState::Eating { .. }) => Color(50, 200, 50), // green
                                Some(BehaviorState::Sleeping { .. }) => Color(100, 100, 200), // blue
                                Some(BehaviorState::FleeHome { .. }) => Color(255, 50, 50),   // red
                                Some(BehaviorState::Idle { .. })
                                | Some(BehaviorState::Wander { .. }) => Color(80, 80, 180), // dim blue
                                Some(BehaviorState::Seek { .. }) => Color(180, 180, 50), // dim yellow
                                _ => fg,
                            }
                        } else {
                            fg
                        }
                    } else {
                        fg
                    }
                } else {
                    fg
                };
                renderer.draw(sx as u16, sy as u16, sprite.ch, fg, None);
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
                renderer.draw(sx as u16, sy as u16, p.ch, p.fg, None);
            }
        }

        // Overlay pass: draw additional markers on top
        if self.overlay == OverlayMode::Resources {
            self.draw_resource_overlay(renderer);
        } else if self.overlay == OverlayMode::Threats {
            self.draw_threat_overlay(renderer);
        } else if self.overlay == OverlayMode::Traffic {
            self.draw_traffic_overlay(renderer);
        } else if self.overlay == OverlayMode::Elevation {
            self.draw_elevation_overlay(renderer);
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

    /// Draw weather effects: rain drops, snowflakes, or fog overlay.
    fn draw_weather(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;

        let is_winter = self.day_night.season == Season::Winter;
        let has_blizzard = self
            .events
            .active_events
            .iter()
            .any(|e| matches!(e, GameEvent::Blizzard { .. }));

        if self.raining || has_blizzard {
            // Scatter weather particles pseudo-randomly across the screen
            // Use tick for animation so particles "fall"
            let density = if has_blizzard { 12 } else { 8 }; // more particles in blizzard
            let panel = PANEL_WIDTH;

            for i in 0..density {
                // Pseudo-random positions using tick + index, shifting each frame
                let seed = self.tick.wrapping_mul(7919).wrapping_add(i as u64 * 6271);
                let sx = ((seed % (w.saturating_sub(panel) as u64)) as u16) + panel;
                let sy = ((seed.wrapping_mul(3) / 7) % h.saturating_sub(status_h) as u64) as u16;

                if sx >= w || sy >= h.saturating_sub(status_h) {
                    continue;
                }

                // Don't render weather in unexplored fog
                let wx = self.camera.x + (sx - panel) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx >= 0 && wy >= 0 && !self.exploration.is_revealed(wx as usize, wy as usize) {
                    continue;
                }

                if is_winter || has_blizzard {
                    // Snow: white dots/asterisks
                    let ch = if i % 3 == 0 { '*' } else { '.' };
                    renderer.draw(sx, sy, ch, Color(220, 230, 255), None);
                } else {
                    // Rain: blue streaks
                    let ch = if i % 2 == 0 { '|' } else { '/' };
                    renderer.draw(sx, sy, ch, Color(100, 140, 200), None);
                }
            }
        }

        // Fog overlay in autumn mornings (light dimming)
        if self.day_night.season == Season::Autumn && self.day_night.is_night() {
            // Very sparse fog wisps
            for i in 0..4 {
                let seed = self.tick.wrapping_mul(4201).wrapping_add(i as u64 * 8731);
                let sx = ((seed % w as u64) as u16).max(PANEL_WIDTH);
                let sy = ((seed.wrapping_mul(5) / 9) % h.saturating_sub(status_h) as u64) as u16;
                if sx < w && sy < h.saturating_sub(status_h) {
                    renderer.draw(sx, sy, '~', Color(180, 180, 190), None);
                }
            }
        }
    }

    fn draw_build_mode(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;
        let (bw, bh) = self.selected_building.size();

        let valid = self.can_place_building(
            self.build_cursor_x,
            self.build_cursor_y,
            self.selected_building,
        );

        // Draw ghost building footprint
        for dy in 0..bh {
            for dx in 0..bw {
                let wx = self.build_cursor_x + dx;
                let wy = self.build_cursor_y + dy;
                let sx = (wx - self.camera.x) * aspect + panel_w;
                let sy = wy - self.camera.y;
                if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
                    for ax in 0..aspect {
                        let cx = sx + ax;
                        if cx >= panel_w && (cx as u16) < w {
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
        let cost = self.selected_building.cost();
        let name = self.selected_building.name();
        let line1 = format!(" BUILD: {} (tab:cycle, enter:place, b/esc:exit) ", name);
        let mut cost_str = String::new();
        if cost.food > 0 {
            cost_str += &format!("F:{} ", cost.food);
        }
        if cost.wood > 0 {
            cost_str += &format!("W:{} ", cost.wood);
        }
        if cost.stone > 0 {
            cost_str += &format!("S:{} ", cost.stone);
        }
        if cost.planks > 0 {
            cost_str += &format!("P:{} ", cost.planks);
        }
        if cost.masonry > 0 {
            cost_str += &format!("M:{} ", cost.masonry);
        }
        let line2 = format!(
            " Cost: {}| Have: F:{} W:{} S:{} P:{} M:{} ",
            cost_str,
            self.resources.food,
            self.resources.wood,
            self.resources.stone,
            self.resources.planks,
            self.resources.masonry
        );
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
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        let sx = (self.query_cx - self.camera.x) * aspect + panel_w;
        let sy = self.query_cy - self.camera.y;

        // Draw cursor bracket across aspect-width cells
        if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
            for dx in 0..aspect {
                let cx = sx + dx;
                if cx >= panel_w && (cx as u16) < w {
                    // Draw a highlight — bright magenta border
                    let cell = renderer.get_cell(cx as u16, sy as u16);
                    let ch = cell.map(|c| c.ch).unwrap_or(' ');
                    renderer.draw(
                        cx as u16,
                        sy as u16,
                        ch,
                        Color(255, 255, 255),
                        Some(Color(180, 0, 180)),
                    );
                }
            }
        }
    }

    fn draw_query_panel(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;

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
                } else {
                    0.0
                };
                if water_depth > 0.0001 {
                    lines.push(format!("water: {:.4}", water_depth));
                }
                let moisture = if ux < self.moisture.width && uy < self.moisture.height {
                    self.moisture.get(ux, uy)
                } else {
                    0.0
                };
                if moisture > 0.01 {
                    lines.push(format!("moisture: {:.2}", moisture));
                }
                let veg = if ux < self.vegetation.width && uy < self.vegetation.height {
                    self.vegetation.get(ux, uy)
                } else {
                    0.0
                };
                if veg > 0.01 {
                    lines.push(format!("vegetation: {:.2}", veg));
                }
                let inf = if ux < self.influence.width && uy < self.influence.height {
                    self.influence.get(ux, uy)
                } else {
                    0.0
                };
                if inf > 0.01 {
                    lines.push(format!("influence: {:.2}", inf));
                }
            } else {
                lines.push(format!("({},{}) out of bounds", wx, wy));
            }
        }

        // Entity info — find all entities at this world position
        for (e, (pos, sprite)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Sprite))>()
            .iter()
        {
            let ex = pos.x.round() as i32;
            let ey = pos.y.round() as i32;
            if ex == wx && ey == wy {
                lines.push("---".to_string());
                lines.push(format!("'{}' at ({:.1},{:.1})", sprite.ch, pos.x, pos.y));

                if let Ok(creature) = self.world.get::<&Creature>(e) {
                    let species_str = match creature.species {
                        Species::Prey => "Prey",
                        Species::Predator => "Predator",
                        Species::Villager => "Villager",
                    };
                    lines.push(species_str.to_string());
                    lines.push(format!("hunger: {:.1}%", creature.hunger * 100.0));
                    lines.push(format!("sight: {:.0}", creature.sight_range));
                    lines.push(format!(
                        "home: ({:.0},{:.0})",
                        creature.home_x, creature.home_y
                    ));
                }
                if let Ok(behavior) = self.world.get::<&Behavior>(e) {
                    let state_str = match &behavior.state {
                        BehaviorState::Wander { timer } => format!("Wander ({})", timer),
                        BehaviorState::Seek {
                            target_x,
                            target_y,
                            reason,
                        } => format!("Seek {:?} ({:.0},{:.0})", reason, target_x, target_y),
                        BehaviorState::Idle { timer } => format!("Idle ({})", timer),
                        BehaviorState::Eating { timer } => format!("Eating ({})", timer),
                        BehaviorState::FleeHome { timer } => format!("Fleeing home! ({})", timer),
                        BehaviorState::AtHome { timer } => format!("At home ({})", timer),
                        BehaviorState::Hunting { target_x, target_y } => {
                            format!("Hunting ({:.0},{:.0})", target_x, target_y)
                        }
                        BehaviorState::Captured => "CAPTURED!".to_string(),
                        BehaviorState::Gathering {
                            timer,
                            resource_type,
                        } => format!("Gathering {:?} ({})", resource_type, timer),
                        BehaviorState::Hauling {
                            target_x,
                            target_y,
                            resource_type,
                        } => format!(
                            "Hauling {:?} ({:.0},{:.0})",
                            resource_type, target_x, target_y
                        ),
                        BehaviorState::Sleeping { timer } => format!("Sleeping ({})", timer),
                        BehaviorState::Building {
                            target_x,
                            target_y,
                            timer,
                        } => format!("Building ({:.0},{:.0}) ({})", target_x, target_y, timer),
                        BehaviorState::Farming {
                            target_x,
                            target_y,
                            lease,
                        } => {
                            format!("Farming ({:.0},{:.0}) [{}]", target_x, target_y, lease)
                        }
                        BehaviorState::Working {
                            target_x,
                            target_y,
                            lease,
                        } => {
                            format!("Working ({:.0},{:.0}) [{}]", target_x, target_y, lease)
                        }
                        BehaviorState::Exploring {
                            target_x,
                            target_y,
                            timer,
                        } => format!("Exploring ({:.0},{:.0}) [{}]", target_x, target_y, timer),
                    };
                    lines.push(format!("state: {}", state_str));
                    lines.push(format!("speed: {:.2}", behavior.speed));
                    match &behavior.state {
                        BehaviorState::Gathering { resource_type, .. }
                        | BehaviorState::Hauling { resource_type, .. } => {
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
                    lines.push(format!(
                        "Farm: {:.0}% grown{}",
                        farm.growth * 100.0,
                        if farm.harvest_ready { " [READY]" } else { "" }
                    ));
                }
                if self.world.get::<&Stockpile>(e).is_ok() {
                    lines.push(format!(
                        "Stockpile (F:{} W:{} S:{})",
                        self.resources.food, self.resources.wood, self.resources.stone
                    ));
                    lines.push(format!(
                        "  Planks:{} Masonry:{} Grain:{}",
                        self.resources.planks, self.resources.masonry, self.resources.grain
                    ));
                }
                if let Ok(pb) = self.world.get::<&ProcessingBuilding>(e) {
                    let recipe_str = match pb.recipe {
                        ecs::Recipe::WoodToPlanks => "2 Wood -> 1 Planks",
                        ecs::Recipe::StoneToMasonry => "2 Stone -> 1 Masonry",
                        ecs::Recipe::FoodToGrain => "3 Food -> 2 Grain",
                        ecs::Recipe::GrainToBread => "2 Grain+1 Wood -> 3 Bread",
                    };
                    let has_input = match pb.recipe {
                        ecs::Recipe::WoodToPlanks => self.resources.wood >= 2,
                        ecs::Recipe::StoneToMasonry => self.resources.stone >= 2,
                        ecs::Recipe::FoodToGrain => self.resources.food >= 3,
                        ecs::Recipe::GrainToBread => {
                            self.resources.grain >= 2 && self.resources.wood >= 1
                        }
                    };
                    let status = if has_input { "ACTIVE" } else { "IDLE" };
                    lines.push(format!("Recipe: {}", recipe_str));
                    lines.push(format!(
                        "Progress: {}/{} [{}]",
                        pb.progress, pb.required, status
                    ));
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
            if sy >= h.saturating_sub(status_h) {
                break;
            }
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
            if sy >= h.saturating_sub(status_h) {
                break;
            }
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
        let status_h = 1u16;
        let base_y = h.saturating_sub(status_h + 1);

        let now = self.tick;
        let visible: Vec<&(u64, String)> = self
            .notifications
            .iter()
            .filter(|(t, _)| now.saturating_sub(*t) < 120)
            .collect();

        for (i, (tick, msg)) in visible.iter().rev().enumerate() {
            let y = base_y.saturating_sub(i as u16);
            if y == 0 {
                break;
            }

            let age = now.saturating_sub(*tick);
            let alpha = if age < 60 {
                1.0
            } else {
                1.0 - (age - 60) as f64 / 60.0
            };
            let brightness = (220.0 * alpha) as u8;

            for (x, ch) in msg.chars().enumerate() {
                if (x as u16) < w {
                    renderer.draw(
                        x as u16,
                        y,
                        ch,
                        Color(brightness, brightness, brightness.min(180)),
                        None,
                    );
                }
            }
        }
    }

    pub(super) fn draw_game_over(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let red = Color(255, 60, 60);
        let white = Color(220, 220, 220);
        let dim = Color(140, 140, 140);

        let lines = [
            ("GAME OVER", red),
            ("", dim),
            ("All villagers have perished.", white),
            ("", dim),
            (
                &format!(
                    "Survived to {} ({} ticks)",
                    self.day_night.date_string(),
                    self.tick
                ),
                dim,
            ),
            (&format!("Peak population: {}", self.peak_population), dim),
            (
                &format!(
                    "Resources: {} food, {} wood, {} stone, {} planks, {} masonry, {} grain",
                    self.resources.food,
                    self.resources.wood,
                    self.resources.stone,
                    self.resources.planks,
                    self.resources.masonry,
                    self.resources.grain
                ),
                dim,
            ),
            ("", dim),
            ("Press [r] to restart, [q] to quit", white),
        ];

        let box_h = lines.len() as u16;
        let box_w: u16 = lines
            .iter()
            .map(|(s, _)| s.len() as u16)
            .max()
            .unwrap_or(30)
            .max(30);
        let start_y = h / 2 - box_h / 2;
        let start_x = w / 2 - box_w / 2;

        for (i, (text, color)) in lines.iter().enumerate() {
            let y = start_y + i as u16;
            if y >= h {
                break;
            }
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
        let fps_str = match self.display_fps {
            Some(fps) => format!("{}fps", fps),
            None => "---".to_string(),
        };
        let pause_str = if self.paused { " PAUSED " } else { "" };
        let status = format!(
            " tick:{}  {}{}  rain:[r]{} erosion:[e]{} time:[t]{} view:[v]{} drain:[d]",
            self.tick,
            fps_str,
            pause_str,
            if self.raining { "+" } else { "-" },
            if self.sim_config.erosion_enabled {
                "+"
            } else {
                "-"
            },
            if self.day_night.enabled { "+" } else { "-" },
            if self.debug_view { "D" } else { "-" },
        );

        for (i, ch) in status.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(
                    i as u16,
                    h - 1,
                    ch,
                    Color(0, 0, 0),
                    Some(Color(180, 180, 180)),
                );
            }
        }
        for i in status.len()..w as usize {
            renderer.draw(
                i as u16,
                h - 1,
                ' ',
                Color(0, 0, 0),
                Some(Color(180, 180, 180)),
            );
        }
    }

    fn draw_resource_overlay(&self, renderer: &mut dyn Renderer) {
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

    fn draw_threat_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        // Collect wolf den positions for danger zone
        let den_positions: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Den)>()
            .iter()
            .map(|(p, _)| (p.x, p.y))
            .collect();

        // Draw danger zone background tint (8 tile radius around dens)
        for sy in 0..h.saturating_sub(status_h) {
            for sx_raw in (panel_w..w as i32).step_by(aspect as usize) {
                let wx = self.camera.x + (sx_raw - panel_w) / aspect;
                let wy = self.camera.y + sy as i32;
                if !self.exploration.is_revealed(wx as usize, wy as usize) {
                    continue;
                }
                let in_danger = den_positions.iter().any(|&(dx, dy)| {
                    let ddx = wx as f64 - dx;
                    let ddy = wy as f64 - dy;
                    ddx * ddx + ddy * ddy < 64.0 // 8 tile radius
                });
                if in_danger {
                    // Draw a dim red tint
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

        // Draw garrison/wall buildings as bright green
        for (pos, _) in self.world.query::<(&Position, &GarrisonBuilding)>().iter() {
            let sx = (pos.x as i32 - self.camera.x) * aspect + panel_w;
            let sy = pos.y as i32 - self.camera.y;
            if sx >= panel_w && sx < w as i32 && sy >= 0 && sy < (h - status_h) as i32 {
                renderer.draw(sx as u16, sy as u16, 'G', Color(50, 255, 50), None);
            }
        }
    }

    fn draw_traffic_overlay(&self, renderer: &mut dyn Renderer) {
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
                    // Intensity scales from dim yellow to bright orange
                    let intensity = (traffic / ROAD_TRAFFIC_THRESHOLD).min(1.0);
                    let r = (80.0 + 175.0 * intensity) as u8;
                    let g = (60.0 + 140.0 * intensity) as u8;
                    let b = (10.0 + 20.0 * intensity) as u8;
                    let ch = if traffic >= ROAD_TRAFFIC_THRESHOLD {
                        '='
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

    /// Draw a minimap in the bottom-right corner (20x10 pixels).
    /// Each pixel represents a chunk of the world map.
    fn draw_minimap(&self, renderer: &mut dyn Renderer) {
        let (scr_w, scr_h) = renderer.size();
        let mini_w: u16 = 20;
        let mini_h: u16 = 10;
        let map_w = self.map.width;
        let map_h = self.map.height;

        // Position in bottom-right, above status line
        let start_x = scr_w.saturating_sub(mini_w + 1);
        let start_y = scr_h.saturating_sub(mini_h + 2);

        let chunk_w = map_w as f64 / mini_w as f64;
        let chunk_h = map_h as f64 / mini_h as f64;

        for my in 0..mini_h {
            for mx in 0..mini_w {
                let world_x = (mx as f64 * chunk_w) as usize;
                let world_y = (my as f64 * chunk_h) as usize;

                // Sample terrain at center of chunk
                let terrain = self.map.get(world_x.min(map_w - 1), world_y.min(map_h - 1));
                let bg = match terrain {
                    Some(Terrain::Water) => Color(30, 60, 140),
                    Some(Terrain::Sand) => Color(160, 140, 80),
                    Some(Terrain::Grass) => Color(40, 100, 40),
                    Some(Terrain::Forest) => Color(20, 60, 20),
                    Some(Terrain::Mountain) => Color(120, 110, 100),
                    Some(Terrain::Snow) => Color(200, 200, 220),
                    Some(Terrain::BuildingFloor) | Some(Terrain::BuildingWall) => {
                        Color(140, 120, 80)
                    }
                    Some(Terrain::Road) => Color(100, 90, 70),
                    Some(Terrain::Cliff) => Color(80, 75, 65),
                    Some(Terrain::Marsh) => Color(30, 70, 50),
                    Some(Terrain::Desert) => Color(190, 170, 110),
                    Some(Terrain::Tundra) => Color(150, 160, 170),
                    Some(Terrain::Scrubland) => Color(120, 110, 55),
                    _ => Color(60, 60, 60),
                };

                let sx = start_x + mx;
                let sy = start_y + my;
                renderer.draw(sx, sy, ' ', Color(0, 0, 0), Some(bg));
            }
        }

        // Draw camera viewport indicator
        let cam_mx = (self.camera.x as f64 / chunk_w) as u16;
        let cam_my = (self.camera.y as f64 / chunk_h) as u16;
        let cam_x = start_x + cam_mx.min(mini_w - 1);
        let cam_y = start_y + cam_my.min(mini_h - 1);
        renderer.draw(
            cam_x,
            cam_y,
            '+',
            Color(255, 255, 255),
            Some(Color(0, 0, 0)),
        );

        // Draw wolves as red dots
        for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
            if creature.species == Species::Predator {
                let wx = (pos.x / chunk_w) as u16;
                let wy = (pos.y / chunk_h) as u16;
                if wx < mini_w && wy < mini_h {
                    renderer.draw(
                        start_x + wx,
                        start_y + wy,
                        '.',
                        Color(255, 50, 50),
                        Some(Color(0, 0, 0)),
                    );
                }
            }
        }
    }

    /// Elevation overlay: brightness = height, white = high, black = low.
    fn draw_elevation_overlay(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH;

        for sy in 0..h.saturating_sub(status_h) {
            for sx in panel_w..w {
                let wx = self.camera.x + (sx - panel_w) as i32 / aspect;
                let wy = self.camera.y + sy as i32;
                if wx < 0 || wy < 0 {
                    continue;
                }
                let ux = wx as usize;
                let uy = wy as usize;
                if ux >= self.map.width || uy >= self.map.height {
                    continue;
                }
                if !self.exploration.is_revealed(ux, uy) {
                    continue;
                }
                let i = uy * self.map.width + ux;
                let hv = self.heights[i];
                let brightness = (hv * 255.0).clamp(0.0, 255.0) as u8;
                renderer.draw(
                    sx,
                    sy,
                    ' ',
                    Color(brightness, brightness, brightness),
                    Some(Color(brightness, brightness, brightness)),
                );
            }
        }
    }

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
                        Terrain::Marsh => ('H', Color(40, 90, 60)),
                        Terrain::Desert => ('D', Color(200, 180, 120)),
                        Terrain::Tundra => ('T', Color(160, 170, 180)),
                        Terrain::Scrubland => ('U', Color(130, 120, 60)),
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
                    && (wx as usize) < self.water.width
                    && (wy as usize) < self.water.height
                {
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
