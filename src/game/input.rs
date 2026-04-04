use crate::ecs::BuildingType;
use crate::renderer::Renderer;

use super::{CELL_ASPECT, OverlayMode, PANEL_WIDTH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    Quit,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    ToggleRain,
    ToggleErosion,
    ToggleDayNight,
    ToggleDebugView,
    TogglePause,
    ToggleQueryMode,
    QueryUp,
    QueryDown,
    QueryLeft,
    QueryRight,
    ToggleBuildMode,
    BuildCycleType,
    BuildPlace,
    BuildUp,
    BuildDown,
    BuildLeft,
    BuildRight,
    Drain,
    Save,
    Load,
    Restart,
    ToggleAutoBuild,
    CycleOverlay,
    GotoSettlement,
    Demolish,
    CycleSpeed,
    /// Advance exactly one sim tick (like '.' in Dwarf Fortress)
    StepOneTick,
    /// Mouse click at screen coordinates (x, y)
    MouseClick {
        x: u16,
        y: u16,
    },
    None,
}

impl super::Game {
    /// Process a single input event, mutating game state accordingly.
    /// Returns without doing simulation or rendering — that happens in step().
    pub(super) fn handle_input(&mut self, input: GameInput, renderer: &mut dyn Renderer) {
        match input {
            GameInput::ScrollUp => self.camera.y -= self.scroll_speed,
            GameInput::ScrollDown => self.camera.y += self.scroll_speed,
            GameInput::ScrollLeft => self.camera.x -= self.scroll_speed,
            GameInput::ScrollRight => self.camera.x += self.scroll_speed,
            GameInput::ToggleRain => {
                self.raining = !self.raining;
                self.dirty.mark_all();
            }
            GameInput::ToggleErosion => {
                self.sim_config.erosion_enabled = !self.sim_config.erosion_enabled
            }
            GameInput::ToggleDayNight => {
                self.day_night.enabled = !self.day_night.enabled;
                self.dirty.mark_all();
            }
            GameInput::ToggleDebugView => {
                self.render_mode = self.render_mode.next();
                self.dirty.mark_all();
                self.notify(format!(
                    "View: {} ({})",
                    self.render_mode.label(),
                    self.render_mode.description()
                ));
            }
            GameInput::TogglePause => self.paused = !self.paused,
            GameInput::ToggleQueryMode => {
                // Mark old cursor position dirty to clean up artifacts
                self.dirty
                    .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                self.query_mode = !self.query_mode;
                if self.query_mode {
                    self.build_mode = false; // mutually exclusive
                    // Center cursor on screen (account for panel)
                    let (vw, vh) = renderer.size();
                    let map_w = vw.saturating_sub(PANEL_WIDTH) as i32;
                    let world_vw = map_w / CELL_ASPECT;
                    self.query_cx = self.camera.x + world_vw / 2;
                    self.query_cy = self.camera.y + vh as i32 / 2;
                }
            }
            GameInput::QueryUp => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cy -= 1;
                }
            }
            GameInput::QueryDown => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cy += 1;
                }
            }
            GameInput::QueryLeft => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cx -= 1;
                }
            }
            GameInput::QueryRight => {
                if self.query_mode {
                    self.dirty
                        .mark(self.query_cx.max(0) as usize, self.query_cy.max(0) as usize);
                    self.query_cx += 1;
                }
            }
            GameInput::ToggleBuildMode => {
                // Mark old cursor footprint dirty to clean up artifacts
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                }
                self.build_mode = !self.build_mode;
                if self.build_mode {
                    self.query_mode = false; // mutually exclusive
                    let (vw, vh) = renderer.size();
                    let map_w = vw.saturating_sub(PANEL_WIDTH) as i32;
                    let world_vw = map_w / CELL_ASPECT;
                    self.build_cursor_x = self.camera.x + world_vw / 2;
                    self.build_cursor_y = self.camera.y + vh as i32 / 2;
                }
            }
            GameInput::BuildUp => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_y -= 1;
                }
            }
            GameInput::BuildDown => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_y += 1;
                }
            }
            GameInput::BuildLeft => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_x -= 1;
                }
            }
            GameInput::BuildRight => {
                if self.build_mode {
                    let (bw, bh) = self.selected_building.size();
                    self.dirty.mark_rect(
                        self.build_cursor_x.max(0) as usize,
                        self.build_cursor_y.max(0) as usize,
                        bw as usize,
                        bh as usize,
                    );
                    self.build_cursor_x += 1;
                }
            }
            GameInput::BuildCycleType => {
                if self.build_mode {
                    let types = BuildingType::all();
                    let idx = types
                        .iter()
                        .position(|t| *t == self.selected_building)
                        .unwrap_or(0);
                    self.selected_building = types[(idx + 1) % types.len()];
                }
            }
            GameInput::BuildPlace => {
                if self.build_mode {
                    self.try_place_building();
                }
            }
            GameInput::Drain => {
                self.water.drain();
                self.pipe_water.drain();
            }
            GameInput::ToggleAutoBuild => self.auto_build = !self.auto_build,
            GameInput::CycleOverlay => {
                self.overlay = match self.overlay {
                    OverlayMode::None => OverlayMode::Tasks,
                    OverlayMode::Tasks => OverlayMode::Resources,
                    OverlayMode::Resources => OverlayMode::Threats,
                    OverlayMode::Threats => OverlayMode::Traffic,
                    OverlayMode::Traffic => OverlayMode::Territory,
                    OverlayMode::Territory => OverlayMode::Wind,
                    OverlayMode::Wind => OverlayMode::WindFlow,
                    OverlayMode::WindFlow => OverlayMode::None,
                };
                self.dirty.mark_all();
            }
            GameInput::MouseClick { x, y } => self.handle_mouse_click(x, y, renderer),
            GameInput::GotoSettlement => {
                let (scx, scy) = self.settlement_center();
                let (vw, vh) = renderer.size();
                let map_cols = vw.saturating_sub(PANEL_WIDTH) as i32 / CELL_ASPECT;
                self.camera.x = scx - map_cols / 2;
                self.camera.y = scy - vh as i32 / 2;
            }
            GameInput::CycleSpeed => {
                self.game_speed = match self.game_speed {
                    1 => 2,
                    2 => 5,
                    5 => 20,
                    _ => 1,
                };
                self.notify(format!("Speed: {}x", self.game_speed));
            }
            GameInput::StepOneTick => {
                // Advance exactly one tick then pause (like '.' in DF)
                self.paused = true;
                // Flag handled below in the sim loop
            }
            GameInput::Demolish => {
                if self.build_mode {
                    self.demolish_at(self.build_cursor_x, self.build_cursor_y);
                }
            }
            GameInput::Save => {
                let _ = self.save("savegame.json");
            }
            GameInput::Load => {} // handled in main.rs loop
            GameInput::Quit | GameInput::Restart | GameInput::None => {}
        }
    }
}
