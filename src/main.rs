mod renderer;
mod crossterm_renderer;
mod headless_renderer;
mod game;
mod ecs;
mod tilemap;

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
                    KeyCode::Up => GameInput::FpsUp,
                    KeyCode::Down => GameInput::FpsDown,
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
    let mut game = Game::new(30);
    run_interactive(&mut game, &mut renderer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use headless_renderer::HeadlessRenderer;

    #[test]
    fn step_advances_tick() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);
        game.step(GameInput::None, &mut r).unwrap();
        assert_eq!(game.tick, 1);
        game.step(GameInput::None, &mut r).unwrap();
        assert_eq!(game.tick, 2);
    }

    #[test]
    fn step_headless_returns_snapshot() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);
        let snap = game.step_headless(GameInput::None, &mut r).unwrap();
        assert_eq!(snap.tick, 1);
        assert_eq!(snap.width, 40);
        assert_eq!(snap.height, 20);
        assert!(snap.text.contains('█'));
        assert_eq!(snap.cells.len(), 20);
        assert_eq!(snap.cells[0].len(), 40);
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let mut r = HeadlessRenderer::new(20, 10);
        let mut game = Game::new(30);
        let snap = game.step_headless(GameInput::None, &mut r).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"tick\":1"));
        assert!(json.contains("\"width\":20"));
        assert!(json.contains("\"height\":10"));
    }

    #[test]
    fn fps_input_changes_target() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);
        game.step(GameInput::FpsUp, &mut r).unwrap();
        assert_eq!(game.target_fps, 35);
        game.step(GameInput::FpsDown, &mut r).unwrap();
        assert_eq!(game.target_fps, 30);
    }

    #[test]
    fn game_draws_block_on_first_tick() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);
        game.tick = 1;
        game.draw(&mut r);

        let frame = r.frame_as_string();
        assert!(frame.contains('█'), "expected block char in frame:\n{}", frame);
    }

    #[test]
    fn game_draws_status_line() {
        let mut r = HeadlessRenderer::new(60, 20);
        let mut game = Game::new(30);
        game.tick = 42;
        game.draw(&mut r);

        let frame = r.frame_as_string();
        assert!(frame.contains("tick: 42"), "expected tick in status line:\n{}", frame);
        assert!(frame.contains("fps: 30"), "expected fps in status line:\n{}", frame);
    }

    #[test]
    fn block_moves_between_ticks() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let snap2 = game.step_headless(GameInput::None, &mut r).unwrap();

        assert_ne!(snap1.text, snap2.text, "frames should differ between ticks");
    }

    #[test]
    fn headless_runs_many_ticks_without_panic() {
        let mut r = HeadlessRenderer::new(80, 24);
        let mut game = Game::new(60);

        for _ in 0..1000 {
            game.step(GameInput::None, &mut r).unwrap();
        }

        assert_eq!(game.tick, 1000);
    }

    #[test]
    fn snapshot_cells_match_text() {
        let mut r = HeadlessRenderer::new(20, 5);
        let mut game = Game::new(30);
        let snap = game.step_headless(GameInput::None, &mut r).unwrap();

        // reconstruct text from cells and compare
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
        let mut game = Game::new(30);
        let script = vec![GameInput::None, GameInput::FpsUp, GameInput::None];
        let snaps = game.run_script(&script, &mut r).unwrap();

        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].tick, 1);
        assert_eq!(snaps[1].tick, 2);
        assert_eq!(snaps[2].tick, 3);
    }

    #[test]
    fn run_script_applies_inputs() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);
        let script = vec![GameInput::FpsUp, GameInput::FpsUp, GameInput::FpsDown];
        game.run_script(&script, &mut r).unwrap();

        assert_eq!(game.target_fps, 35); // 30 +5 +5 -5
    }

    #[test]
    fn frame_diff_detects_changes() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let snap2 = game.step_headless(GameInput::None, &mut r).unwrap();

        let diff = snap1.diff(&snap2);
        assert_eq!(diff.from_tick, 1);
        assert_eq!(diff.to_tick, 2);
        assert!(!diff.changes.is_empty(), "block moved, so there should be changes");
    }

    #[test]
    fn frame_diff_is_empty_for_identical_frames() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = Game::new(30);

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let diff = snap1.diff(&snap1);
        assert!(diff.changes.is_empty(), "diffing a frame against itself should be empty");
    }

    #[test]
    fn frame_diff_serializes_to_json() {
        let mut r = HeadlessRenderer::new(20, 10);
        let mut game = Game::new(30);

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let snap2 = game.step_headless(GameInput::None, &mut r).unwrap();

        let diff = snap1.diff(&snap2);
        let json = serde_json::to_string(&diff).unwrap();
        assert!(json.contains("\"from_tick\":1"));
        assert!(json.contains("\"to_tick\":2"));
        assert!(json.contains("\"changes\""));
    }
}
