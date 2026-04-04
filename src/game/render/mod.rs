mod debug;
mod landscape;
mod map;
mod normal;
mod overlays;
mod query;
mod shared;

use super::{CELL_ASPECT, GameEvent, OverlayMode, PANEL_WIDTH, ROAD_TRAFFIC_THRESHOLD};
use crate::ecs::{
    Behavior, BehaviorState, BuildingType, Den, FarmPlot, FoodSource, GarrisonBuilding,
    ProcessingBuilding, ResourceType, SeekReason, Species, Sprite, Stockpile, StoneDeposit,
    TownHallBuilding, Velocity,
};
use crate::renderer::{Color, Renderer};
use crate::simulation::Season;
use crate::tilemap::{Terrain, blend_vegetation};

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
        let ch1 = Terrain::Grass.landscape_ch(10, 20, 0.5);
        let ch2 = Terrain::Grass.landscape_ch(10, 20, 0.5);
        assert_eq!(ch1, ch2);
        // Different vegetation levels produce different chars
        let bare = Terrain::Grass.landscape_ch(10, 20, 0.0);
        let dense = Terrain::Grass.landscape_ch(10, 20, 0.9);
        // bare should be sparser char than dense (not necessarily different at every position)
        let _ = (bare, dense);
        // Just check it doesn't panic for a range
        for x in 0..20 {
            for y in 0..20 {
                let _ = Terrain::Mountain.landscape_ch(x, y, 0.5);
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
