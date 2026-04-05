use super::{
    BuildingType, CELL_ASPECT, Color, GameEvent, OverlayMode, PANEL_WIDTH, Renderer, Season,
    Terrain, map_building_center_glyph,
};
use crate::ecs::{self, Creature, Position, Species, Stockpile};

impl super::super::Game {
    /// Draw the left-side UI panel.
    pub(in super::super) fn draw_panel(&self, renderer: &mut dyn Renderer) {
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
            OverlayMode::Wind => "WIND",
            OverlayMode::WindFlow => "WIND FLOW",
            OverlayMode::Height => "HEIGHT",
            OverlayMode::Discharge => "DISCHARGE",
            OverlayMode::Moisture => "MOISTURE",
            OverlayMode::Slope => "SLOPE",
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

    /// Draw building center glyphs for Normal and Landscape modes.
    /// Overlays the building-type glyph on the center tile of each completed
    /// building so the player can identify building types at a glance.
    pub(in super::super) fn draw_building_center_overlays(
        &self,
        renderer: &mut dyn Renderer,
        apply_lighting: bool,
    ) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH;

        // Helper closure to draw one building center glyph
        let mut draw_center = |pos: &Position, bt: &BuildingType| {
            let (bw, bh) = bt.size();
            let cx = pos.x.round() as i32 + bw / 2;
            let cy = pos.y.round() as i32 + bh / 2;
            if !self.exploration.is_revealed(cx as usize, cy as usize) {
                return;
            }
            let sx_i = (cx - self.camera.x) * aspect + panel_w as i32;
            let sy_i = cy - self.camera.y;
            if sx_i >= panel_w as i32
                && sy_i >= 0
                && (sx_i as u16) < w
                && (sy_i as u16) < h.saturating_sub(status_h)
            {
                let (glyph, mut color) = map_building_center_glyph(bt);
                // Brighten the glyph color for visibility in Normal/Landscape modes
                color = Color(
                    (color.0 as u16 + 40).min(255) as u8,
                    (color.1 as u16 + 40).min(255) as u8,
                    (color.2 as u16 + 40).min(255) as u8,
                );
                if apply_lighting {
                    color = self
                        .day_night
                        .apply_lighting(color, cx as usize, cy as usize);
                }
                // Use the building floor bg for contrast
                let bg = if apply_lighting {
                    self.day_night.apply_lighting_bg(
                        Terrain::BuildingFloor.bg(),
                        cx as usize,
                        cy as usize,
                    )
                } else {
                    Some(Terrain::BuildingFloor.bg().unwrap_or(Color(100, 80, 60)))
                };
                renderer.draw(sx_i as u16, sy_i as u16, glyph, color, bg);
            }
        };

        for (pos, _) in self.world.query::<(&Position, &Stockpile)>().iter() {
            draw_center(pos, &BuildingType::Stockpile);
        }
        for (pos, _) in self.world.query::<(&Position, &ecs::HutBuilding)>().iter() {
            draw_center(pos, &BuildingType::Hut);
        }
        for (pos, _) in self
            .world
            .query::<(&Position, &ecs::GarrisonBuilding)>()
            .iter()
        {
            draw_center(pos, &BuildingType::Garrison);
        }
        for (pos, _) in self
            .world
            .query::<(&Position, &ecs::TownHallBuilding)>()
            .iter()
        {
            draw_center(pos, &BuildingType::TownHall);
        }
        for (pos, _) in self
            .world
            .query::<(&Position, &ecs::ShelterBuilding)>()
            .iter()
        {
            draw_center(pos, &BuildingType::Shelter);
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
            draw_center(pos, &bt);
        }
    }

    /// Draw weather effects: rain drops, snowflakes, or fog overlay.
    pub(in super::super) fn draw_weather(&self, renderer: &mut dyn Renderer) {
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

    pub(in super::super) fn draw_build_mode(&self, renderer: &mut dyn Renderer) {
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

    pub(in super::super) fn draw_notifications(&self, renderer: &mut dyn Renderer) {
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

    pub(in super::super) fn draw_game_over(&self, renderer: &mut dyn Renderer) {
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

    pub(in super::super) fn draw_status(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let fps_str = match self.display_fps {
            Some(fps) => format!("{}fps", fps),
            None => "---".to_string(),
        };
        let pause_str = if self.paused { " PAUSED " } else { "" };
        let status = format!(
            " tick:{}  {}{}  rain:[r]{} erosion:[e]{} time:[t]{} view:[v]{} drain:[d] [k]query [b]uild",
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
            self.render_mode.label(),
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

    /// Draw a minimap in the bottom-right corner (20x10 pixels).
    /// Each pixel represents a chunk of the world map.
    pub(in super::super) fn draw_minimap(&self, renderer: &mut dyn Renderer) {
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
                    Some(Terrain::Stump) => Color(80, 70, 40),
                    Some(Terrain::Bare) => Color(70, 65, 45),
                    Some(Terrain::Sapling) => Color(35, 90, 35),
                    Some(Terrain::Quarry) => Color(90, 80, 70),
                    Some(Terrain::QuarryDeep) => Color(65, 58, 50),
                    Some(Terrain::ScarredGround) => Color(115, 105, 90),
                    Some(Terrain::Mountain) => Color(120, 110, 100),
                    Some(Terrain::Snow) => Color(200, 200, 220),
                    Some(Terrain::BuildingFloor) | Some(Terrain::BuildingWall) => {
                        Color(140, 120, 80)
                    }
                    Some(Terrain::Road) => Color(100, 90, 70),
                    Some(Terrain::Ford) => Color(80, 140, 220),
                    Some(Terrain::Bridge) => Color(140, 100, 50),
                    Some(Terrain::Burning) => Color(255, 120, 20),
                    Some(Terrain::Scorched) => Color(60, 50, 40),
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

    /// Unified water check + rendering. Returns Some((ch, fg, bg)) if this tile
    /// should render as water, None if it's dry land.
    ///
    /// ONE code path for ALL water: ocean, rivers, rain puddles, flooding.
    /// Reads from pipe_water.depth only — the single source of truth for water.
    /// Discharge field provides a secondary tint for river channels.
    pub(in super::super) fn water_visual(
        &self,
        wx: usize,
        wy: usize,
        tick: u64,
    ) -> Option<(char, Color, Color)> {
        let depth = self.pipe_water.get_depth(wx, wy);
        if depth < 0.005 {
            return None;
        }

        let intensity = (depth * 4.0).clamp(0.0, 1.0);

        // Discharge tint: high-flow channels get slightly different hue
        let idx = wy * self.map.width + wx;
        let discharge_alpha = if idx < self.hydro.discharge.len() {
            crate::hydrology::erf_approx(0.4 * self.hydro.discharge[idx]).min(0.5)
        } else {
            0.0
        };

        // Base water color — deeper = darker blue, discharge adds grey-blue tint
        let r = (20.0 * (1.0 - intensity) + discharge_alpha * 30.0) as u8;
        let g = (60.0 + 60.0 * intensity + discharge_alpha * 40.0) as u8;
        let b = (140.0 + 80.0 * intensity) as u8;
        let fg = Color(r, g, b);

        let bg_r = (10.0 * (1.0 - intensity) + discharge_alpha * 15.0) as u8;
        let bg_g = (30.0 + 30.0 * intensity + discharge_alpha * 20.0) as u8;
        let bg_b = (90.0 + 50.0 * intensity) as u8;
        let bg = Color(bg_r, bg_g, bg_b);

        // Animated water character
        let water_chars = ['~', '≈', '∼'];
        let anim = ((tick / 8) as usize + wx + wy) % 3;
        let ch = water_chars[anim];

        Some((ch, fg, bg))
    }
}
