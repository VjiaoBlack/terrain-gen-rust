use super::{CELL_ASPECT, GameEvent, OverlayMode, PANEL_WIDTH, ROAD_TRAFFIC_THRESHOLD};
use crate::ecs::{
    self, Behavior, BehaviorState, BuildingType, Creature, Den, FarmPlot, FoodSource,
    GarrisonBuilding, Position, ProcessingBuilding, ResourceType, SeekReason, Species, Sprite,
    Stockpile, StoneDeposit, TownHallBuilding, Velocity,
};
use crate::renderer::{Color, Renderer};
use crate::simulation::Season;
use crate::tilemap::Terrain;

/// Compute directional character from a movement vector.
/// Returns `>`, `<`, `^`, or `v` based on the dominant axis.
/// Falls back to `>` when the vector is zero-length.
fn direction_char(dx: f64, dy: f64) -> char {
    if dx.abs() < 0.001 && dy.abs() < 0.001 {
        return '>';
    }
    if dx.abs() > dy.abs() {
        if dx > 0.0 { '>' } else { '<' }
    } else {
        // Screen Y increases downward, so positive dy = down
        if dy > 0.0 { 'v' } else { '^' }
    }
}

/// Map (Species, BehaviorState, velocity) to a display character and base color.
///
/// This replaces the static `sprite.ch` so entities visually reflect their
/// current activity. The returned color is the *base* color before day/night
/// tinting is applied by the caller.
pub(super) fn entity_visual(
    species: Species,
    state: &BehaviorState,
    vel_dx: f64,
    vel_dy: f64,
    _default_sprite: &Sprite,
) -> (char, Color) {
    match species {
        Species::Villager => villager_visual(state, vel_dx, vel_dy),
        Species::Predator => predator_visual(state, vel_dx, vel_dy),
        Species::Prey => prey_visual(state),
    }
}

fn villager_visual(state: &BehaviorState, vdx: f64, vdy: f64) -> (char, Color) {
    match state {
        BehaviorState::Idle { .. } => ('o', Color(80, 80, 180)),
        BehaviorState::Wander { .. } => ('o', Color(100, 100, 160)),
        BehaviorState::Seek { reason, .. } => match reason {
            SeekReason::Food => ('!', Color(220, 180, 50)),
            SeekReason::Stockpile => (direction_char(vdx, vdy), Color(200, 180, 50)),
            SeekReason::BuildSite => (direction_char(vdx, vdy), Color(255, 220, 50)),
            SeekReason::Wood => (direction_char(vdx, vdy), Color(139, 90, 43)),
            SeekReason::Stone => (direction_char(vdx, vdy), Color(150, 150, 150)),
            SeekReason::Hut => (direction_char(vdx, vdy), Color(100, 100, 200)),
            SeekReason::ExitBuilding => ('V', Color(100, 200, 255)),
            SeekReason::Unknown => ('?', Color(150, 150, 50)),
        },
        BehaviorState::Gathering { resource_type, .. } => match resource_type {
            ResourceType::Wood => ('T', Color(139, 90, 43)),
            ResourceType::Stone => ('M', Color(150, 150, 150)),
            ResourceType::Food => ('F', Color(50, 200, 50)),
            _ => ('g', Color(180, 180, 100)),
        },
        BehaviorState::Hauling { resource_type, .. } => {
            let dir = direction_char(vdx, vdy);
            match resource_type {
                ResourceType::Wood => (dir, Color(180, 120, 50)),
                ResourceType::Stone => (dir, Color(180, 180, 180)),
                ResourceType::Food => (dir, Color(100, 220, 80)),
                ResourceType::Grain => (dir, Color(220, 200, 80)),
                ResourceType::Planks => (dir, Color(200, 160, 80)),
                ResourceType::Masonry => (dir, Color(200, 200, 210)),
            }
        }
        BehaviorState::Building { .. } => ('B', Color(255, 220, 50)),
        BehaviorState::Farming { .. } => ('~', Color(80, 200, 80)),
        BehaviorState::Working { .. } => ('*', Color(200, 120, 50)),
        BehaviorState::Exploring { .. } => ('>', Color(50, 180, 255)),
        BehaviorState::Sleeping { .. } => ('z', Color(60, 60, 140)),
        BehaviorState::FleeHome { .. } => ('!', Color(255, 50, 50)),
        BehaviorState::Eating { .. } => ('e', Color(50, 200, 50)),
        // Villagers shouldn't normally reach these, but handle gracefully
        BehaviorState::Hunting { .. } => ('V', Color(200, 100, 100)),
        BehaviorState::Captured => ('X', Color(200, 50, 50)),
        BehaviorState::AtHome { .. } => ('.', Color(80, 80, 100)),
    }
}

fn predator_visual(state: &BehaviorState, vdx: f64, vdy: f64) -> (char, Color) {
    match state {
        BehaviorState::Wander { .. } => ('w', Color(160, 50, 50)),
        BehaviorState::Idle { .. } => ('w', Color(120, 40, 40)),
        BehaviorState::Seek { .. } => (direction_char(vdx, vdy), Color(180, 60, 60)),
        BehaviorState::Hunting { .. } => ('W', Color(255, 50, 50)),
        BehaviorState::Eating { .. } => ('X', Color(200, 30, 30)),
        BehaviorState::FleeHome { .. } => (direction_char(vdx, vdy), Color(160, 80, 80)),
        _ => ('W', Color(160, 50, 50)),
    }
}

fn prey_visual(state: &BehaviorState) -> (char, Color) {
    match state {
        BehaviorState::Wander { .. } => ('r', Color(180, 140, 80)),
        BehaviorState::Idle { .. } => ('r', Color(140, 110, 60)),
        BehaviorState::Eating { .. } => ('r', Color(100, 180, 60)),
        BehaviorState::FleeHome { .. } => ('!', Color(255, 200, 50)),
        BehaviorState::AtHome { .. } => ('.', Color(100, 80, 50)),
        BehaviorState::Captured => ('x', Color(200, 50, 50)),
        _ => ('r', Color(180, 140, 80)),
    }
}

// ---------------------------------------------------------------------------
// Map Mode entity visuals: bright, saturated glyphs on muted terrain.
// These intentionally differ from entity_visual() — Map Mode uses its own
// glyph language (design doc: pillar4_observable/map_rendering_mode.md).
// ---------------------------------------------------------------------------

/// Map Mode glyph + color for an entity, keyed on (species, behavior_state).
/// Returns `None` for entities that should be hidden (e.g. prey AtHome).
pub(super) fn map_mode_entity_visual(
    species: Species,
    state: &BehaviorState,
) -> Option<(char, Color)> {
    match species {
        Species::Villager => Some(map_villager_visual(state)),
        Species::Predator => Some(map_predator_visual(state)),
        Species::Prey => map_prey_visual(state),
    }
}

fn map_villager_visual(state: &BehaviorState) -> (char, Color) {
    match state {
        BehaviorState::Idle { .. } | BehaviorState::Wander { .. } => ('@', Color(80, 200, 255)),
        BehaviorState::Seek { .. } => ('@', Color(180, 200, 100)),
        BehaviorState::Gathering { resource_type, .. } => match resource_type {
            ResourceType::Wood => ('$', Color(160, 110, 50)),
            ResourceType::Stone => ('$', Color(150, 150, 160)),
            ResourceType::Food => ('$', Color(80, 200, 80)),
            _ => ('$', Color(180, 180, 100)),
        },
        BehaviorState::Hauling { .. } => ('%', Color(220, 190, 60)),
        BehaviorState::Building { .. } => ('&', Color(255, 220, 50)),
        BehaviorState::Farming { .. } => ('f', Color(80, 200, 80)),
        BehaviorState::Working { .. } => ('g', Color(210, 140, 60)),
        BehaviorState::Sleeping { .. } => ('z', Color(100, 100, 180)),
        BehaviorState::FleeHome { .. } => ('!', Color(255, 60, 60)),
        BehaviorState::Exploring { .. } => ('?', Color(160, 220, 160)),
        BehaviorState::Captured => ('x', Color(120, 30, 30)),
        BehaviorState::Eating { .. } => ('@', Color(80, 200, 255)),
        BehaviorState::Hunting { .. } => ('@', Color(80, 200, 255)),
        BehaviorState::AtHome { .. } => ('@', Color(80, 200, 255)),
    }
}

fn map_predator_visual(state: &BehaviorState) -> (char, Color) {
    match state {
        BehaviorState::Hunting { .. } => ('W', Color(255, 40, 40)),
        _ => ('w', Color(200, 50, 50)),
    }
}

fn map_prey_visual(state: &BehaviorState) -> Option<(char, Color)> {
    match state {
        BehaviorState::AtHome { .. } => None, // hidden in den
        BehaviorState::Eating { .. } => Some(('r', Color(140, 200, 90))),
        BehaviorState::FleeHome { .. } => Some(('!', Color(255, 150, 50))),
        BehaviorState::Captured => Some(('x', Color(200, 50, 50))),
        _ => Some(('r', Color(190, 155, 90))),
    }
}

// ---------------------------------------------------------------------------
// Landscape Mode entity visuals: simple glyphs, saturated colors by state.
// Color carries meaning, not the glyph. Entities pop against muted terrain.
// ---------------------------------------------------------------------------

/// Landscape Mode glyph + color for an entity. Returns `None` for hidden entities.
pub(super) fn landscape_entity_visual(
    species: Species,
    state: &BehaviorState,
) -> Option<(char, Color)> {
    match species {
        Species::Villager => Some(landscape_villager_visual(state)),
        Species::Predator => Some(landscape_predator_visual(state)),
        Species::Prey => landscape_prey_visual(state),
    }
}

fn landscape_villager_visual(state: &BehaviorState) -> (char, Color) {
    match state {
        BehaviorState::Idle { .. } | BehaviorState::Wander { .. } => {
            ('o', Color(240, 220, 180)) // warm cream
        }
        BehaviorState::Seek { .. } => ('o', Color(200, 200, 150)),
        BehaviorState::Gathering { .. } => ('o', Color(180, 220, 120)), // spring green
        BehaviorState::Hauling { .. } => ('o', Color(200, 200, 140)),   // laden, slightly dim
        BehaviorState::Building { .. } => ('o', Color(220, 180, 100)),  // warm amber
        BehaviorState::Farming { .. } => ('o', Color(140, 200, 100)),   // earthy green
        BehaviorState::Working { .. } => ('o', Color(220, 180, 100)),   // warm amber
        BehaviorState::Sleeping { .. } => ('o', Color(120, 120, 160)),  // cool, dormant
        BehaviorState::FleeHome { .. } => ('o', Color(255, 100, 80)),   // alarm red
        BehaviorState::Exploring { .. } => ('o', Color(180, 220, 240)), // sky blue
        BehaviorState::Eating { .. } => ('o', Color(240, 220, 180)),
        BehaviorState::Captured => ('x', Color(200, 50, 50)),
        BehaviorState::Hunting { .. } => ('o', Color(240, 220, 180)),
        BehaviorState::AtHome { .. } => ('o', Color(120, 120, 140)),
    }
}

fn landscape_predator_visual(state: &BehaviorState) -> (char, Color) {
    match state {
        BehaviorState::Hunting { .. } => ('w', Color(255, 60, 60)), // aggressive red
        _ => ('w', Color(220, 60, 60)),                             // red
    }
}

fn landscape_prey_visual(state: &BehaviorState) -> Option<(char, Color)> {
    match state {
        BehaviorState::AtHome { .. } => None, // hidden in den
        BehaviorState::Captured => Some(('x', Color(200, 50, 50))),
        _ => Some(('r', Color(200, 200, 180))), // soft, neutral
    }
}

/// Map Mode glyph for a building type's center marker tile.
fn map_building_center_glyph(bt: &BuildingType) -> (char, Color) {
    match bt {
        BuildingType::Hut => ('\u{2302}', Color(170, 140, 100)), // ⌂
        BuildingType::Stockpile => ('\u{25A0}', Color(190, 150, 60)), // ■
        BuildingType::Farm => ('\u{2261}', Color(90, 180, 60)),  // ≡
        BuildingType::Workshop => ('\u{2699}', Color(200, 180, 110)), // ⚙
        BuildingType::Smithy => ('\u{2206}', Color(200, 100, 40)), // ∆
        BuildingType::Garrison => ('\u{2694}', Color(180, 50, 50)), // ⚔
        BuildingType::Granary => ('G', Color(200, 180, 80)),
        BuildingType::Bakery => ('B', Color(210, 160, 90)),
        BuildingType::TownHall => ('H', Color(255, 220, 60)),
        BuildingType::Wall => ('#', Color(170, 150, 120)),
        BuildingType::Road => ('=', Color(170, 145, 90)),
        BuildingType::Bridge => ('#', Color(140, 100, 50)),
        BuildingType::Shelter => ('\u{2302}', Color(140, 110, 70)), // ⌂ (dimmer than Hut)
    }
}

impl super::Game {
    /// Apply seasonal color tinting to vegetation-sensitive terrain.
    fn season_tint(&self, color: Color, terrain: &Terrain) -> Color {
        use crate::simulation::Season;
        match terrain {
            Terrain::Grass | Terrain::Forest | Terrain::Sapling => {
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

    /// Apply worn terrain visual based on traffic level. Returns `Some((ch, fg, bg))`
    /// if traffic is high enough to visually alter the tile, `None` otherwise.
    /// This makes supply lines visible without activating overlays.
    ///
    /// Traffic tiers:
    /// - 10-50 (Faint): darken background by 10%, keep terrain char
    /// - 50-150 (Worn): replace char with '.' or ',', shift color toward tan
    /// - 150-300 (Trail): oriented trail char based on dominant travel direction
    pub(super) fn worn_terrain_override(
        &self,
        wx: usize,
        wy: usize,
        base_ch: char,
        base_fg: Color,
        base_bg: Color,
    ) -> (char, Color, Color) {
        let traffic = self.traffic.get(wx, wy);
        if traffic < 10.0 || traffic >= ROAD_TRAFFIC_THRESHOLD {
            return (base_ch, base_fg, base_bg);
        }

        if traffic < 50.0 {
            // Faint tier: darken background by 10%
            let dim = 0.9;
            let bg = Color(
                (base_bg.0 as f64 * dim) as u8,
                (base_bg.1 as f64 * dim) as u8,
                (base_bg.2 as f64 * dim) as u8,
            );
            (base_ch, base_fg, bg)
        } else if traffic < 150.0 {
            // Worn tier: dot trail characters, shift color toward tan
            let t = (traffic - 50.0) / 100.0; // 0..1 within tier
            let ch = if ((wx + wy) % 3) == 0 { ',' } else { '.' };
            let fg = Color(
                (base_fg.0 as f64 * (1.0 - t) + 160.0 * t) as u8,
                (base_fg.1 as f64 * (1.0 - t) + 140.0 * t) as u8,
                (base_fg.2 as f64 * (1.0 - t) + 100.0 * t) as u8,
            );
            let bg = Color(
                (base_bg.0 as f64 * (1.0 - t * 0.3) + 60.0 * t * 0.3) as u8,
                (base_bg.1 as f64 * (1.0 - t * 0.3) + 50.0 * t * 0.3) as u8,
                (base_bg.2 as f64 * (1.0 - t * 0.3) + 30.0 * t * 0.3) as u8,
            );
            (ch, fg, bg)
        } else {
            // Trail tier (150-300): oriented trail char based on dominant direction
            let ch = self.traffic.trail_char(wx, wy);
            let fg = Color(140, 110, 70); // tan-brown
            let bg = Color(
                (base_bg.0 as f64 * 0.7 + 50.0 * 0.3) as u8,
                (base_bg.1 as f64 * 0.7 + 40.0 * 0.3) as u8,
                (base_bg.2 as f64 * 0.7 + 25.0 * 0.3) as u8,
            );
            (ch, fg, bg)
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
                            let fg = self.season_tint(terrain.fg(), terrain);
                            let bg = terrain.bg().map(|c| self.season_tint(c, terrain));
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
                        let (ch, fg, bg) =
                            self.map_terrain_glyph(terrain, wx as usize, wy as usize);
                        // Apply worn terrain visual from foot traffic
                        let (ch, fg, bg) =
                            self.worn_terrain_override(wx as usize, wy as usize, ch, fg, bg);
                        renderer.draw(sx, sy, ch, fg, Some(bg));
                    }
                }
            }
        }

        // --- Building center markers (on top of terrain) ---
        // Completed buildings with marker components:
        for (pos, _stockpile) in self.world.query::<(&Position, &Stockpile)>().iter() {
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
                        let (ch, fg, bg) =
                            self.landscape_terrain_glyph(terrain, wx as usize, wy as usize);
                        // Apply worn terrain visual from foot traffic
                        let (ch, fg, bg) =
                            self.worn_terrain_override(wx as usize, wy as usize, ch, fg, bg);
                        renderer.draw(sx, sy, ch, fg, Some(bg));
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
        // Base texture character from position hash
        let mut ch = terrain.landscape_ch(wx, wy);
        let mut fg = terrain.landscape_fg();
        let mut bg = terrain.landscape_bg();

        // Vegetation overlay: dense vegetation overrides base texture
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
                    fg = Color(18, 65, 15);
                    bg = Color(10, 45, 8);
                } else if v > 0.5 {
                    // Brush, young trees
                    let pool: &[char] = &['%', ':', '"', ';'];
                    let idx = (wx.wrapping_mul(7).wrapping_add(wy.wrapping_mul(13))) % pool.len();
                    ch = pool[idx];
                    fg = Color(30, 95, 28);
                    bg = Color(18, 60, 15);
                } else if v > 0.2 {
                    // Light scrub
                    let pool: &[char] = &['"', ',', '\'', ';'];
                    let idx = (wx.wrapping_mul(7).wrapping_add(wy.wrapping_mul(13))) % pool.len();
                    ch = pool[idx];
                    fg = Color(48, 125, 42);
                    bg = Color(28, 78, 24);
                }
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

    /// Draw building center glyphs for Normal and Landscape modes.
    /// Overlays the building-type glyph on the center tile of each completed
    /// building so the player can identify building types at a glance.
    fn draw_building_center_overlays(&self, renderer: &mut dyn Renderer, apply_lighting: bool) {
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
                            target_x, target_y, ..
                        } => {
                            format!("Farming ({:.0},{:.0})", target_x, target_y)
                        }
                        BehaviorState::Working {
                            target_x, target_y, ..
                        } => {
                            format!("Working ({:.0},{:.0})", target_x, target_y)
                        }
                        BehaviorState::Exploring {
                            target_x,
                            target_y,
                            timer,
                        } => {
                            format!("Exploring ({:.0},{:.0}) ({})", target_x, target_y, timer)
                        }
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
                    let fert = self.soil_fertility.get(farm.tile_x, farm.tile_y);
                    if farm.fallow {
                        lines.push(format!("Farm: FALLOW (fertility {:.0}%)", fert * 100.0));
                    } else {
                        lines.push(format!(
                            "Farm: {:.0}% grown{}",
                            farm.growth * 100.0,
                            if farm.harvest_ready { " [READY]" } else { "" }
                        ));
                        lines.push(format!("  fertility: {:.0}%", fert * 100.0));
                    }
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
                    && (wx as usize) < self.water.width
                    && (wy as usize) < self.water.height
                    && self.dirty.is_dirty(wx as usize, wy as usize)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::{BehaviorState, ResourceType, SeekReason, Species};
    use crate::renderer::Color;

    fn dummy_sprite() -> Sprite {
        Sprite {
            ch: 'V',
            fg: Color(100, 200, 255),
        }
    }

    // --- direction_char ---

    #[test]
    fn direction_char_right() {
        assert_eq!(direction_char(1.0, 0.0), '>');
    }

    #[test]
    fn direction_char_left() {
        assert_eq!(direction_char(-1.0, 0.0), '<');
    }

    #[test]
    fn direction_char_down() {
        assert_eq!(direction_char(0.0, 1.0), 'v');
    }

    #[test]
    fn direction_char_up() {
        assert_eq!(direction_char(0.0, -1.0), '^');
    }

    #[test]
    fn direction_char_zero_defaults_right() {
        assert_eq!(direction_char(0.0, 0.0), '>');
    }

    #[test]
    fn direction_char_diagonal_favors_dominant() {
        // dx=3, dy=1 => horizontal dominant => '>'
        assert_eq!(direction_char(3.0, 1.0), '>');
        // dx=1, dy=-3 => vertical dominant => '^'
        assert_eq!(direction_char(1.0, -3.0), '^');
    }

    // --- Villager states ---

    #[test]
    fn villager_idle_shows_circle() {
        let state = BehaviorState::Idle { timer: 10 };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'o');
        assert_eq!(color, Color(80, 80, 180));
    }

    #[test]
    fn villager_wander_shows_circle_slate() {
        let state = BehaviorState::Wander { timer: 5 };
        let (ch, _) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'o');
    }

    #[test]
    fn villager_seek_food_shows_exclamation() {
        let state = BehaviorState::Seek {
            target_x: 10.0,
            target_y: 10.0,
            reason: SeekReason::Food,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 1.0, 0.0, &dummy_sprite());
        assert_eq!(ch, '!');
        assert_eq!(color, Color(220, 180, 50));
    }

    #[test]
    fn villager_seek_stockpile_shows_direction() {
        let state = BehaviorState::Seek {
            target_x: 10.0,
            target_y: 10.0,
            reason: SeekReason::Stockpile,
        };
        // Moving right
        let (ch, _) = entity_visual(Species::Villager, &state, 2.0, 0.5, &dummy_sprite());
        assert_eq!(ch, '>');
        // Moving left
        let (ch, _) = entity_visual(Species::Villager, &state, -2.0, 0.5, &dummy_sprite());
        assert_eq!(ch, '<');
    }

    #[test]
    fn villager_seek_wood_brown() {
        let state = BehaviorState::Seek {
            target_x: 5.0,
            target_y: 5.0,
            reason: SeekReason::Wood,
        };
        let (_, color) = entity_visual(Species::Villager, &state, 1.0, 0.0, &dummy_sprite());
        assert_eq!(color, Color(139, 90, 43));
    }

    #[test]
    fn villager_gathering_wood_shows_t() {
        let state = BehaviorState::Gathering {
            timer: 20,
            resource_type: ResourceType::Wood,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'T');
        assert_eq!(color, Color(139, 90, 43));
    }

    #[test]
    fn villager_gathering_stone_shows_m() {
        let state = BehaviorState::Gathering {
            timer: 15,
            resource_type: ResourceType::Stone,
        };
        let (ch, _) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'M');
    }

    #[test]
    fn villager_gathering_food_shows_f() {
        let state = BehaviorState::Gathering {
            timer: 10,
            resource_type: ResourceType::Food,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'F');
        assert_eq!(color, Color(50, 200, 50));
    }

    #[test]
    fn villager_hauling_uses_direction_and_resource_color() {
        let state = BehaviorState::Hauling {
            target_x: 10.0,
            target_y: 10.0,
            resource_type: ResourceType::Wood,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 1.0, &dummy_sprite());
        assert_eq!(ch, 'v'); // moving down
        assert_eq!(color, Color(180, 120, 50));
    }

    #[test]
    fn villager_hauling_stone_grey() {
        let state = BehaviorState::Hauling {
            target_x: 0.0,
            target_y: 0.0,
            resource_type: ResourceType::Stone,
        };
        let (_, color) = entity_visual(Species::Villager, &state, -1.0, 0.0, &dummy_sprite());
        assert_eq!(color, Color(180, 180, 180));
    }

    #[test]
    fn villager_building_shows_b() {
        let state = BehaviorState::Building {
            target_x: 5.0,
            target_y: 5.0,
            timer: 30,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'B');
        assert_eq!(color, Color(255, 220, 50));
    }

    #[test]
    fn villager_farming_shows_tilde() {
        let state = BehaviorState::Farming {
            target_x: 5.0,
            target_y: 5.0,
            lease: 100,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, '~');
        assert_eq!(color, Color(80, 200, 80));
    }

    #[test]
    fn villager_working_shows_star() {
        let state = BehaviorState::Working {
            target_x: 5.0,
            target_y: 5.0,
            lease: 50,
        };
        let (ch, _) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, '*');
    }

    #[test]
    fn villager_exploring_shows_arrow() {
        let state = BehaviorState::Exploring {
            target_x: 20.0,
            target_y: 20.0,
            timer: 100,
        };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, '>');
        assert_eq!(color, Color(50, 180, 255));
    }

    #[test]
    fn villager_sleeping_shows_z() {
        let state = BehaviorState::Sleeping { timer: 200 };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'z');
        assert_eq!(color, Color(60, 60, 140));
    }

    #[test]
    fn villager_flee_shows_exclamation_red() {
        let state = BehaviorState::FleeHome { timer: 50 };
        let (ch, color) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, '!');
        assert_eq!(color, Color(255, 50, 50));
    }

    #[test]
    fn villager_eating_shows_e() {
        let state = BehaviorState::Eating { timer: 10 };
        let (ch, _) = entity_visual(Species::Villager, &state, 0.0, 0.0, &dummy_sprite());
        assert_eq!(ch, 'e');
    }

    // --- Predator (wolf) states ---

    #[test]
    fn wolf_wander_lowercase() {
        let state = BehaviorState::Wander { timer: 10 };
        let sprite = Sprite {
            ch: 'W',
            fg: Color(160, 50, 50),
        };
        let (ch, _) = entity_visual(Species::Predator, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, 'w');
    }

    #[test]
    fn wolf_hunting_uppercase_bright_red() {
        let state = BehaviorState::Hunting {
            target_x: 5.0,
            target_y: 5.0,
        };
        let sprite = Sprite {
            ch: 'W',
            fg: Color(160, 50, 50),
        };
        let (ch, color) = entity_visual(Species::Predator, &state, 1.0, 0.0, &sprite);
        assert_eq!(ch, 'W');
        assert_eq!(color, Color(255, 50, 50));
    }

    #[test]
    fn wolf_eating_shows_x() {
        let state = BehaviorState::Eating { timer: 5 };
        let sprite = Sprite {
            ch: 'W',
            fg: Color(160, 50, 50),
        };
        let (ch, _) = entity_visual(Species::Predator, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, 'X');
    }

    #[test]
    fn wolf_seek_directional() {
        let state = BehaviorState::Seek {
            target_x: 10.0,
            target_y: 10.0,
            reason: SeekReason::Unknown,
        };
        let sprite = Sprite {
            ch: 'W',
            fg: Color(160, 50, 50),
        };
        let (ch, _) = entity_visual(Species::Predator, &state, -1.0, 0.0, &sprite);
        assert_eq!(ch, '<');
    }

    // --- Prey (rabbit) states ---

    #[test]
    fn rabbit_wander_lowercase_r() {
        let state = BehaviorState::Wander { timer: 10 };
        let sprite = Sprite {
            ch: 'r',
            fg: Color(180, 140, 80),
        };
        let (ch, _) = entity_visual(Species::Prey, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, 'r');
    }

    #[test]
    fn rabbit_eating_green_tint() {
        let state = BehaviorState::Eating { timer: 5 };
        let sprite = Sprite {
            ch: 'r',
            fg: Color(180, 140, 80),
        };
        let (ch, color) = entity_visual(Species::Prey, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, 'r');
        assert_eq!(color, Color(100, 180, 60));
    }

    #[test]
    fn rabbit_flee_exclamation_yellow() {
        let state = BehaviorState::FleeHome { timer: 10 };
        let sprite = Sprite {
            ch: 'r',
            fg: Color(180, 140, 80),
        };
        let (ch, color) = entity_visual(Species::Prey, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, '!');
        assert_eq!(color, Color(255, 200, 50));
    }

    #[test]
    fn rabbit_captured_lowercase_x_red() {
        let state = BehaviorState::Captured;
        let sprite = Sprite {
            ch: 'r',
            fg: Color(180, 140, 80),
        };
        let (ch, color) = entity_visual(Species::Prey, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, 'x');
        assert_eq!(color, Color(200, 50, 50));
    }

    #[test]
    fn rabbit_at_home_dot() {
        let state = BehaviorState::AtHome { timer: 100 };
        let sprite = Sprite {
            ch: 'r',
            fg: Color(180, 140, 80),
        };
        let (ch, _) = entity_visual(Species::Prey, &state, 0.0, 0.0, &sprite);
        assert_eq!(ch, '.');
    }

    // --- New hardening tests ---

    #[test]
    fn villager_states_produce_different_chars() {
        let s = dummy_sprite();
        let idle_ch = entity_visual(
            Species::Villager,
            &BehaviorState::Idle { timer: 10 },
            0.0,
            0.0,
            &s,
        )
        .0;
        let sleep_ch = entity_visual(
            Species::Villager,
            &BehaviorState::Sleeping { timer: 10 },
            0.0,
            0.0,
            &s,
        )
        .0;
        let build_ch = entity_visual(
            Species::Villager,
            &BehaviorState::Building {
                target_x: 0.0,
                target_y: 0.0,
                timer: 10,
            },
            0.0,
            0.0,
            &s,
        )
        .0;
        let farm_ch = entity_visual(
            Species::Villager,
            &BehaviorState::Farming {
                target_x: 0.0,
                target_y: 0.0,
                lease: 10,
            },
            0.0,
            0.0,
            &s,
        )
        .0;

        // Each state should produce a distinct character
        let chars = [idle_ch, sleep_ch, build_ch, farm_ch];
        for i in 0..chars.len() {
            for j in (i + 1)..chars.len() {
                assert_ne!(
                    chars[i], chars[j],
                    "states {} and {} should have different chars: '{}' vs '{}'",
                    i, j, chars[i], chars[j]
                );
            }
        }
    }

    #[test]
    fn wolf_hunting_vs_idle_different_chars() {
        let wolf_sprite = Sprite {
            ch: 'W',
            fg: Color(160, 50, 50),
        };
        let (hunt_ch, hunt_color) = entity_visual(
            Species::Predator,
            &BehaviorState::Hunting {
                target_x: 5.0,
                target_y: 5.0,
            },
            1.0,
            0.0,
            &wolf_sprite,
        );
        let (idle_ch, idle_color) = entity_visual(
            Species::Predator,
            &BehaviorState::Idle { timer: 10 },
            0.0,
            0.0,
            &wolf_sprite,
        );

        assert_ne!(
            hunt_ch, idle_ch,
            "hunting wolf should look different from idle wolf"
        );
        assert_ne!(
            hunt_color, idle_color,
            "hunting wolf color should differ from idle"
        );
    }

    #[test]
    fn direction_arrows_correct_for_cardinal_movements() {
        // Moving right
        assert_eq!(direction_char(1.0, 0.0), '>');
        // Moving left
        assert_eq!(direction_char(-1.0, 0.0), '<');
        // Moving up (screen coords: negative Y = up)
        assert_eq!(direction_char(0.0, -1.0), '^');
        // Moving down
        assert_eq!(direction_char(0.0, 1.0), 'v');
    }

    #[test]
    fn villager_hauling_different_resources_same_direction_different_colors() {
        let s = dummy_sprite();
        let (_, wood_color) = entity_visual(
            Species::Villager,
            &BehaviorState::Hauling {
                target_x: 10.0,
                target_y: 0.0,
                resource_type: ResourceType::Wood,
            },
            1.0,
            0.0,
            &s,
        );
        let (_, stone_color) = entity_visual(
            Species::Villager,
            &BehaviorState::Hauling {
                target_x: 10.0,
                target_y: 0.0,
                resource_type: ResourceType::Stone,
            },
            1.0,
            0.0,
            &s,
        );
        let (_, food_color) = entity_visual(
            Species::Villager,
            &BehaviorState::Hauling {
                target_x: 10.0,
                target_y: 0.0,
                resource_type: ResourceType::Food,
            },
            1.0,
            0.0,
            &s,
        );

        assert_ne!(
            wood_color, stone_color,
            "wood and stone haul colors should differ"
        );
        assert_ne!(
            wood_color, food_color,
            "wood and food haul colors should differ"
        );
        assert_ne!(
            stone_color, food_color,
            "stone and food haul colors should differ"
        );
    }

    #[test]
    fn villager_captured_shows_x() {
        let s = dummy_sprite();
        let (ch, _) = entity_visual(Species::Villager, &BehaviorState::Captured, 0.0, 0.0, &s);
        assert_eq!(ch, 'X');
    }

    // --- Map Mode entity visuals ---

    #[test]
    fn map_villager_idle_shows_at_sign() {
        let (ch, color) =
            map_mode_entity_visual(Species::Villager, &BehaviorState::Idle { timer: 10 }).unwrap();
        assert_eq!(ch, '@');
        assert_eq!(color, Color(80, 200, 255));
    }

    #[test]
    fn map_villager_gathering_wood_shows_dollar() {
        let state = BehaviorState::Gathering {
            resource_type: ResourceType::Wood,
            timer: 5,
        };
        let (ch, color) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, '$');
        assert_eq!(color, Color(160, 110, 50));
    }

    #[test]
    fn map_villager_hauling_shows_percent() {
        let state = BehaviorState::Hauling {
            resource_type: ResourceType::Stone,
            target_x: 0.0,
            target_y: 0.0,
        };
        let (ch, _) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, '%');
    }

    #[test]
    fn map_villager_building_shows_ampersand() {
        let state = BehaviorState::Building {
            target_x: 0.0,
            target_y: 0.0,
            timer: 0,
        };
        let (ch, _) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, '&');
    }

    #[test]
    fn map_villager_farming_shows_f() {
        let state = BehaviorState::Farming {
            target_x: 0.0,
            target_y: 0.0,
            lease: 5,
        };
        let (ch, _) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, 'f');
    }

    #[test]
    fn map_villager_working_shows_g() {
        let state = BehaviorState::Working {
            target_x: 0.0,
            target_y: 0.0,
            lease: 3,
        };
        let (ch, _) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, 'g');
    }

    #[test]
    fn map_villager_sleeping_shows_z() {
        let state = BehaviorState::Sleeping { timer: 10 };
        let (ch, _) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, 'z');
    }

    #[test]
    fn map_villager_fleeing_shows_exclamation() {
        let state = BehaviorState::FleeHome { timer: 10 };
        let (ch, color) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, '!');
        assert_eq!(color, Color(255, 60, 60));
    }

    #[test]
    fn map_villager_exploring_shows_question() {
        let state = BehaviorState::Exploring {
            target_x: 0.0,
            target_y: 0.0,
            timer: 0,
        };
        let (ch, _) = map_mode_entity_visual(Species::Villager, &state).unwrap();
        assert_eq!(ch, '?');
    }

    #[test]
    fn map_predator_hunting_uppercase_w() {
        let state = BehaviorState::Hunting {
            target_x: 0.0,
            target_y: 0.0,
        };
        let (ch, _) = map_mode_entity_visual(Species::Predator, &state).unwrap();
        assert_eq!(ch, 'W');
    }

    #[test]
    fn map_predator_wander_lowercase_w() {
        let state = BehaviorState::Wander { timer: 5 };
        let (ch, _) = map_mode_entity_visual(Species::Predator, &state).unwrap();
        assert_eq!(ch, 'w');
    }

    #[test]
    fn map_prey_at_home_hidden() {
        let state = BehaviorState::AtHome { timer: 5 };
        assert!(map_mode_entity_visual(Species::Prey, &state).is_none());
    }

    #[test]
    fn map_prey_fleeing_shows_orange_exclamation() {
        let state = BehaviorState::FleeHome { timer: 10 };
        let (ch, color) = map_mode_entity_visual(Species::Prey, &state).unwrap();
        assert_eq!(ch, '!');
        assert_eq!(color, Color(255, 150, 50));
    }

    #[test]
    fn map_mode_glyph_uniqueness_terrain_vs_entity() {
        // Verify that key entity glyphs (@, $, %, &, f, g, z, !) are not used
        // as terrain glyphs (except documented exceptions like # for wall/cliff).
        use crate::tilemap::Terrain;
        let entity_chars = ['@', '$', '%', '&', 'f', 'g', 'z', 'w', 'W', 'r'];
        let terrains = [
            Terrain::Water,
            Terrain::Sand,
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Mountain,
            Terrain::Snow,
            Terrain::Cliff,
            Terrain::Marsh,
            Terrain::Desert,
            Terrain::Tundra,
            Terrain::Scrubland,
            Terrain::Road,
            Terrain::BuildingFloor,
            Terrain::BuildingWall,
        ];
        for t in &terrains {
            let tch = t.map_ch();
            for &ech in &entity_chars {
                assert_ne!(
                    tch, ech,
                    "terrain {:?} map glyph '{}' conflicts with entity glyph '{}'",
                    t, tch, ech
                );
            }
        }
    }

    #[test]
    fn map_mode_behavior_coverage() {
        // Every BehaviorState variant must produce a result for every Species.
        // We only test Villager here since it has the most states.
        let states: Vec<BehaviorState> = vec![
            BehaviorState::Idle { timer: 0 },
            BehaviorState::Wander { timer: 0 },
            BehaviorState::Seek {
                target_x: 0.0,
                target_y: 0.0,
                reason: SeekReason::Food,
            },
            BehaviorState::Gathering {
                resource_type: ResourceType::Wood,
                timer: 0,
            },
            BehaviorState::Hauling {
                resource_type: ResourceType::Wood,
                target_x: 0.0,
                target_y: 0.0,
            },
            BehaviorState::Building {
                target_x: 0.0,
                target_y: 0.0,
                timer: 0,
            },
            BehaviorState::Farming {
                target_x: 0.0,
                target_y: 0.0,
                lease: 0,
            },
            BehaviorState::Working {
                target_x: 0.0,
                target_y: 0.0,
                lease: 0,
            },
            BehaviorState::Sleeping { timer: 0 },
            BehaviorState::FleeHome { timer: 0 },
            BehaviorState::Exploring {
                target_x: 0.0,
                target_y: 0.0,
                timer: 0,
            },
            BehaviorState::Captured,
            BehaviorState::Eating { timer: 0 },
            BehaviorState::Hunting {
                target_x: 0.0,
                target_y: 0.0,
            },
            BehaviorState::AtHome { timer: 0 },
        ];
        for state in &states {
            let result = map_mode_entity_visual(Species::Villager, state);
            assert!(
                result.is_some(),
                "Villager should have a map glyph for {:?}",
                state
            );
        }
    }

    #[test]
    fn map_building_center_glyphs_distinct() {
        use crate::ecs::BuildingType;
        // Center markers for major buildings should all be distinct
        let buildings = [
            BuildingType::Hut,
            BuildingType::Stockpile,
            BuildingType::Farm,
            BuildingType::Workshop,
            BuildingType::Smithy,
            BuildingType::Garrison,
            BuildingType::Granary,
            BuildingType::Bakery,
            BuildingType::TownHall,
        ];
        let glyphs: Vec<char> = buildings
            .iter()
            .map(|b| map_building_center_glyph(b).0)
            .collect();
        for i in 0..glyphs.len() {
            for j in (i + 1)..glyphs.len() {
                assert_ne!(
                    glyphs[i], glyphs[j],
                    "building {:?} and {:?} share glyph '{}'",
                    buildings[i], buildings[j], glyphs[i]
                );
            }
        }
    }

    #[test]
    fn render_mode_cycle() {
        use super::super::RenderMode;
        assert_eq!(RenderMode::Normal.next(), RenderMode::Map);
        assert_eq!(RenderMode::Map.next(), RenderMode::Landscape);
        assert_eq!(RenderMode::Landscape.next(), RenderMode::Debug);
        assert_eq!(RenderMode::Debug.next(), RenderMode::Normal);
    }

    #[test]
    fn render_mode_labels() {
        use super::super::RenderMode;
        assert_eq!(RenderMode::Normal.label(), "Normal");
        assert_eq!(RenderMode::Map.label(), "Map");
        assert_eq!(RenderMode::Landscape.label(), "Landscape");
        assert_eq!(RenderMode::Debug.label(), "Debug");
    }

    // --- Landscape Mode tests ---

    #[test]
    fn landscape_texture_pool_coverage() {
        use crate::tilemap::Terrain;
        // Every terrain type should return a non-empty texture pool
        let terrains = [
            Terrain::Water,
            Terrain::Sand,
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Mountain,
            Terrain::Snow,
            Terrain::Cliff,
            Terrain::Marsh,
            Terrain::Desert,
            Terrain::Tundra,
            Terrain::Scrubland,
            Terrain::Stump,
            Terrain::Bare,
            Terrain::Sapling,
            Terrain::Quarry,
            Terrain::QuarryDeep,
            Terrain::ScarredGround,
            Terrain::BuildingFloor,
            Terrain::BuildingWall,
            Terrain::Road,
            Terrain::Ford,
            Terrain::Bridge,
            Terrain::Ice,
            Terrain::FloodWater,
            Terrain::Burning,
            Terrain::Scorched,
        ];
        for t in &terrains {
            let pool = t.landscape_texture_pool();
            assert!(!pool.is_empty(), "empty texture pool for {:?}", t);
        }
    }

    #[test]
    fn landscape_ch_deterministic() {
        use crate::tilemap::Terrain;
        // Same position should always produce the same character
        let ch1 = Terrain::Grass.landscape_ch(10, 20);
        let ch2 = Terrain::Grass.landscape_ch(10, 20);
        assert_eq!(ch1, ch2);
        // Different positions should (usually) produce different characters
        // Just check it doesn't panic for a range
        for x in 0..20 {
            for y in 0..20 {
                let _ = Terrain::Mountain.landscape_ch(x, y);
            }
        }
    }

    #[test]
    fn landscape_fg_bg_low_contrast() {
        use crate::tilemap::Terrain;
        // For landscape mode, fg and bg should be close (low contrast).
        // Check that the RGB distance is bounded for key terrain types.
        let terrains = [
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Sand,
            Terrain::Mountain,
            Terrain::Snow,
            Terrain::Water,
        ];
        for t in &terrains {
            let fg = t.landscape_fg();
            let bg = t.landscape_bg();
            let dr = (fg.0 as i32 - bg.0 as i32).abs();
            let dg = (fg.1 as i32 - bg.1 as i32).abs();
            let db = (fg.2 as i32 - bg.2 as i32).abs();
            let max_diff = dr.max(dg).max(db);
            assert!(
                max_diff <= 100,
                "landscape fg/bg too far apart for {:?}: fg={:?} bg={:?} max_diff={}",
                t,
                fg,
                bg,
                max_diff
            );
        }
    }

    #[test]
    fn landscape_entity_villager_all_states() {
        // Every villager behavior state should produce a visible glyph
        let states: Vec<BehaviorState> = vec![
            BehaviorState::Idle { timer: 10 },
            BehaviorState::Wander { timer: 10 },
            BehaviorState::Seek {
                target_x: 0.0,
                target_y: 0.0,
                reason: SeekReason::Food,
            },
            BehaviorState::Gathering {
                timer: 10,
                resource_type: ResourceType::Wood,
            },
            BehaviorState::Hauling {
                target_x: 0.0,
                target_y: 0.0,
                resource_type: ResourceType::Wood,
            },
            BehaviorState::Building {
                target_x: 0.0,
                target_y: 0.0,
                timer: 10,
            },
            BehaviorState::Farming {
                target_x: 0.0,
                target_y: 0.0,
                lease: 10,
            },
            BehaviorState::Working {
                target_x: 0.0,
                target_y: 0.0,
                lease: 10,
            },
            BehaviorState::Sleeping { timer: 10 },
            BehaviorState::FleeHome { timer: 10 },
            BehaviorState::Exploring {
                target_x: 0.0,
                target_y: 0.0,
                timer: 10,
            },
            BehaviorState::Eating { timer: 10 },
        ];
        for state in &states {
            let result = landscape_entity_visual(Species::Villager, state);
            assert!(
                result.is_some(),
                "Villager should have a landscape glyph for {:?}",
                state
            );
            let (ch, _) = result.unwrap();
            assert_eq!(ch, 'o', "Villager glyph should be 'o' in landscape mode");
        }
    }

    #[test]
    fn landscape_entity_predator_saturated() {
        // Predator should always have red-ish high-saturation color
        let state = BehaviorState::Hunting {
            target_x: 0.0,
            target_y: 0.0,
        };
        let (ch, color) = landscape_entity_visual(Species::Predator, &state).unwrap();
        assert_eq!(ch, 'w');
        assert!(color.0 > 200, "predator red channel should be high");
    }

    #[test]
    fn landscape_prey_hidden_at_home() {
        let state = BehaviorState::AtHome { timer: 10 };
        assert!(landscape_entity_visual(Species::Prey, &state).is_none());
    }
}
