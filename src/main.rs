mod renderer;
mod crossterm_renderer;
mod headless_renderer;
mod game;
mod ecs;
mod tilemap;
mod terrain_gen;
mod simulation;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use std::time::{Duration, Instant};

use crossterm_renderer::CrosstermRenderer;
use game::{Game, GameInput};

fn run_interactive(game: &mut Game, renderer: &mut CrosstermRenderer) -> Result<()> {
    loop {
        let frame_start = Instant::now();

        // input
        let input = if event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(KeyEvent { code, .. }) => match code {
                    KeyCode::Char('q') | KeyCode::Esc => GameInput::Quit,
                    KeyCode::Up => GameInput::ScrollUp,
                    KeyCode::Down => GameInput::ScrollDown,
                    KeyCode::Left => GameInput::ScrollLeft,
                    KeyCode::Right => GameInput::ScrollRight,
                    KeyCode::Char('r') => GameInput::ToggleRain,
                    KeyCode::Char('e') => GameInput::ToggleErosion,
                    KeyCode::Char('t') => GameInput::ToggleDayNight,
                    KeyCode::Char('d') => GameInput::Drain,
                    _ => GameInput::None,
                },
                Event::Resize(w, h) => {
                    renderer.resize(w, h);
                    GameInput::None
                }
                _ => GameInput::None,
            }
        } else {
            GameInput::None
        };

        if input == GameInput::Quit {
            return Ok(());
        }

        game.step(input, renderer)?;

        // sleep to hit target fps
        let target = Duration::from_secs_f64(1.0 / game.target_fps as f64);
        let elapsed = frame_start.elapsed();
        if let Some(remaining) = target.checked_sub(elapsed) {
            std::thread::sleep(remaining);
        }
    }
}

fn main() -> Result<()> {
    let mut renderer = CrosstermRenderer::new()?;
    let mut game = Game::new(30, 42);
    run_interactive(&mut game, &mut renderer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use headless_renderer::HeadlessRenderer;

    fn test_game() -> Game {
        Game::new(30, 42)
    }

    #[test]
    fn step_advances_tick() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        game.step(GameInput::None, &mut r).unwrap();
        assert_eq!(game.tick, 1);
        game.step(GameInput::None, &mut r).unwrap();
        assert_eq!(game.tick, 2);
    }

    #[test]
    fn step_headless_returns_snapshot() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        let snap = game.step_headless(GameInput::None, &mut r).unwrap();
        assert_eq!(snap.tick, 1);
        assert_eq!(snap.width, 40);
        assert_eq!(snap.height, 20);
        assert_eq!(snap.cells.len(), 20);
        assert_eq!(snap.cells[0].len(), 40);
        // frame should contain terrain chars, not be blank
        let non_blank = snap.text.chars().filter(|c| *c != ' ' && *c != '\n').count();
        assert!(non_blank > 0, "frame should have terrain content");
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let mut r = HeadlessRenderer::new(20, 10);
        let mut game = test_game();
        let snap = game.step_headless(GameInput::None, &mut r).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"tick\":1"));
        assert!(json.contains("\"width\":20"));
        assert!(json.contains("\"height\":10"));
    }

    #[test]
    fn scroll_moves_camera() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        let start_x = game.camera.x;
        let start_y = game.camera.y;

        game.step(GameInput::ScrollRight, &mut r).unwrap();
        assert!(game.camera.x > start_x);

        game.step(GameInput::ScrollDown, &mut r).unwrap();
        assert!(game.camera.y > start_y);
    }

    #[test]
    fn terrain_renders_on_frame() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        game.step(GameInput::None, &mut r).unwrap();

        let frame = r.frame_as_string();
        // should contain terrain characters
        let terrain_chars = ['~', '·', '\'', ':', '^'];
        let has_terrain = frame.chars().any(|c| terrain_chars.contains(&c));
        assert!(has_terrain, "frame should contain terrain characters:\n{}", frame);
    }

    #[test]
    fn game_draws_status_line() {
        let mut r = HeadlessRenderer::new(60, 20);
        let mut game = test_game();
        game.step(GameInput::None, &mut r).unwrap();

        let frame = r.frame_as_string();
        assert!(frame.contains("tick: 1"), "expected tick in status line:\n{}", frame);
        assert!(frame.contains("cam:"), "expected camera pos in status line:\n{}", frame);
    }

    #[test]
    fn entities_move_between_ticks() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let snap2 = game.step_headless(GameInput::None, &mut r).unwrap();

        let diff = snap1.diff(&snap2);
        assert!(!diff.changes.is_empty(), "NPC movement should cause frame changes");
    }

    #[test]
    fn headless_runs_many_ticks_without_panic() {
        let mut r = HeadlessRenderer::new(80, 24);
        let mut game = test_game();

        for _ in 0..1000 {
            game.step(GameInput::None, &mut r).unwrap();
        }

        assert_eq!(game.tick, 1000);
    }

    #[test]
    fn snapshot_cells_match_text() {
        let mut r = HeadlessRenderer::new(20, 5);
        let mut game = test_game();
        let snap = game.step_headless(GameInput::None, &mut r).unwrap();

        let mut from_cells = String::new();
        for (y, row) in snap.cells.iter().enumerate() {
            for cell in row {
                from_cells.push(cell.ch);
            }
            if y < snap.cells.len() - 1 {
                from_cells.push('\n');
            }
        }
        assert_eq!(snap.text, from_cells);
    }

    #[test]
    fn run_script_returns_all_snapshots() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        let script = vec![GameInput::None, GameInput::ScrollRight, GameInput::None];
        let snaps = game.run_script(&script, &mut r).unwrap();

        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].tick, 1);
        assert_eq!(snaps[1].tick, 2);
        assert_eq!(snaps[2].tick, 3);
    }

    #[test]
    fn frame_diff_is_empty_for_identical_frames() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let diff = snap1.diff(&snap1);
        assert!(diff.changes.is_empty(), "diffing a frame against itself should be empty");
    }

    #[test]
    fn toggle_rain_starts_water() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        assert!(!game.raining);

        game.step(GameInput::ToggleRain, &mut r).unwrap();
        assert!(game.raining);

        // run some ticks with rain
        for _ in 0..50 {
            game.step(GameInput::None, &mut r).unwrap();
        }

        // check that water appeared somewhere on the map
        let mut has_water = false;
        for y in 0..game.water.height {
            for x in 0..game.water.width {
                if game.water.get(x, y) > 0.0 {
                    has_water = true;
                    break;
                }
            }
            if has_water { break; }
        }
        assert!(has_water, "rain should add water to the map");
    }

    #[test]
    fn drain_removes_water() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();

        // rain, then stop rain, then drain
        game.step(GameInput::ToggleRain, &mut r).unwrap();
        for _ in 0..20 {
            game.step(GameInput::None, &mut r).unwrap();
        }
        game.step(GameInput::ToggleRain, &mut r).unwrap(); // stop rain
        game.step(GameInput::Drain, &mut r).unwrap();

        let total: f64 = (0..game.water.height)
            .flat_map(|y| (0..game.water.width).map(move |x| (x, y)))
            .map(|(x, y)| game.water.get(x, y))
            .sum();
        assert_eq!(total, 0.0, "drain should remove all water");
    }

    #[test]
    fn toggle_erosion() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();
        assert!(!game.sim_config.erosion_enabled);

        game.step(GameInput::ToggleErosion, &mut r).unwrap();
        assert!(game.sim_config.erosion_enabled);

        game.step(GameInput::ToggleErosion, &mut r).unwrap();
        assert!(!game.sim_config.erosion_enabled);
    }

    #[test]
    fn status_line_shows_rain_state() {
        let mut r = HeadlessRenderer::new(80, 20);
        let mut game = test_game();

        game.step(GameInput::None, &mut r).unwrap();
        let frame = r.frame_as_string();
        assert!(frame.contains("rain: [r] off"), "should show rain off:\n{}", frame);

        game.step(GameInput::ToggleRain, &mut r).unwrap();
        let frame = r.frame_as_string();
        assert!(frame.contains("rain: [r] ON"), "should show rain ON:\n{}", frame);
    }

    #[test]
    fn toggle_day_night() {
        let mut r = HeadlessRenderer::new(100, 20);
        let mut game = test_game();
        assert!(game.day_night.enabled);

        game.step(GameInput::None, &mut r).unwrap();
        let frame = r.frame_as_string();
        assert!(frame.contains("time: [t] ON"), "should show time ON:\n{}", frame);

        game.step(GameInput::ToggleDayNight, &mut r).unwrap();
        assert!(!game.day_night.enabled);
        let frame = r.frame_as_string();
        assert!(frame.contains("time: [t] off"), "should show time off:\n{}", frame);
    }

    #[test]
    fn day_night_affects_colors() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();

        // Noon: bright
        game.day_night.hour = 12.0;
        game.step(GameInput::None, &mut r).unwrap();
        let noon_snap = game.step_headless(GameInput::None, &mut r).unwrap();

        // Midnight: dark
        game.day_night.hour = 0.0;
        let midnight_snap = game.step_headless(GameInput::None, &mut r).unwrap();

        // Compare brightness of a terrain cell (not status bar)
        let noon_cell = &noon_snap.cells[5][5];
        let midnight_cell = &midnight_snap.cells[5][5];

        let noon_brightness = noon_cell.fg.0 as u32 + noon_cell.fg.1 as u32 + noon_cell.fg.2 as u32;
        let midnight_brightness = midnight_cell.fg.0 as u32 + midnight_cell.fg.1 as u32 + midnight_cell.fg.2 as u32;

        assert!(noon_brightness > midnight_brightness,
            "noon should be brighter than midnight: noon={} midnight={}", noon_brightness, midnight_brightness);
    }

    #[test]
    fn frame_diff_serializes_to_json() {
        let mut r = HeadlessRenderer::new(20, 10);
        let mut game = test_game();

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let snap2 = game.step_headless(GameInput::None, &mut r).unwrap();

        let diff = snap1.diff(&snap2);
        let json = serde_json::to_string(&diff).unwrap();
        assert!(json.contains("\"from_tick\":1"));
        assert!(json.contains("\"to_tick\":2"));
        assert!(json.contains("\"changes\""));
    }
}
