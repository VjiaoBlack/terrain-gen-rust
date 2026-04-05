mod crossterm_renderer;

use terrain_gen_rust::game;
use terrain_gen_rust::headless_renderer;
use terrain_gen_rust::renderer::{self, Renderer};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};

use crossterm_renderer::CrosstermRenderer;
use game::{Game, GameInput};

fn map_key(code: KeyCode, query_mode: bool, build_mode: bool, game_over: bool) -> GameInput {
    if game_over {
        return match code {
            KeyCode::Char('q') | KeyCode::Esc => GameInput::Quit,
            KeyCode::Char('r') => GameInput::Restart,
            _ => GameInput::None,
        };
    }
    if query_mode {
        // In query mode: WASD moves cursor, arrows still scroll camera
        match code {
            KeyCode::Char('q') | KeyCode::Esc => GameInput::ToggleQueryMode, // exit query mode
            KeyCode::Char('w') => GameInput::QueryUp,
            KeyCode::Char('s') => GameInput::QueryDown,
            KeyCode::Char('a') => GameInput::QueryLeft,
            KeyCode::Char('d') => GameInput::QueryRight,
            KeyCode::Up => GameInput::ScrollUp,
            KeyCode::Down => GameInput::ScrollDown,
            KeyCode::Left => GameInput::ScrollLeft,
            KeyCode::Right => GameInput::ScrollRight,
            _ => GameInput::None,
        }
    } else if build_mode {
        // In build mode: WASD moves build cursor, arrows scroll camera
        match code {
            KeyCode::Char('b') | KeyCode::Esc => GameInput::ToggleBuildMode, // exit build mode
            KeyCode::Char('w') => GameInput::BuildUp,
            KeyCode::Char('s') => GameInput::BuildDown,
            KeyCode::Char('a') => GameInput::BuildLeft,
            KeyCode::Char('d') => GameInput::BuildRight,
            KeyCode::Tab => GameInput::BuildCycleType,
            KeyCode::Enter | KeyCode::Char(' ') => GameInput::BuildPlace,
            KeyCode::Char('x') => GameInput::Demolish,
            KeyCode::Up => GameInput::ScrollUp,
            KeyCode::Down => GameInput::ScrollDown,
            KeyCode::Left => GameInput::ScrollLeft,
            KeyCode::Right => GameInput::ScrollRight,
            _ => GameInput::None,
        }
    } else {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => GameInput::Quit,
            KeyCode::Up => GameInput::ScrollUp,
            KeyCode::Down => GameInput::ScrollDown,
            KeyCode::Left => GameInput::ScrollLeft,
            KeyCode::Right => GameInput::ScrollRight,
            KeyCode::Char('r') => GameInput::ToggleRain,
            KeyCode::Char('e') => GameInput::ToggleErosion,
            KeyCode::Char('t') => GameInput::ToggleDayNight,
            KeyCode::Char('v') => GameInput::ToggleDebugView,
            KeyCode::Char('k') => GameInput::ToggleQueryMode,
            KeyCode::Char('b') => GameInput::ToggleBuildMode,
            KeyCode::Char('a') => GameInput::ToggleAutoBuild,
            KeyCode::Char('o') => GameInput::CycleOverlay,
            KeyCode::Char('g') => GameInput::GotoSettlement,
            KeyCode::Char('f') => GameInput::CycleSpeed,
            KeyCode::Char(' ') => GameInput::TogglePause,
            KeyCode::Char('.') => GameInput::StepOneTick,
            KeyCode::Char('d') => GameInput::Drain,
            KeyCode::Char('s') => GameInput::Save,
            KeyCode::Char('l') => GameInput::Load,
            _ => GameInput::None,
        }
    }
}

/// Returns Ok(true) to restart, Ok(false) to quit.
fn run_interactive(game: &mut Game, renderer: &mut CrosstermRenderer) -> Result<bool> {
    let mut fps_timer = Instant::now();
    let mut frame_count = 0u32;
    let mut display_fps = 0u32;
    let mut quit_pending = false;

    loop {
        let frame_start = Instant::now();

        // Drain all pending events — handles key repeat and avoids input lag
        let mut input = GameInput::None;
        while event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(KeyEvent { code, .. }) => {
                    let mapped = map_key(code, game.query_mode, game.build_mode, game.game_over);
                    // Prioritize quit; for movement, take the latest
                    if mapped == GameInput::Quit {
                        input = GameInput::Quit;
                        break;
                    }
                    if mapped != GameInput::None {
                        input = mapped;
                    }
                }
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    ..
                }) => {
                    input = GameInput::MouseClick { x: column, y: row };
                }
                Event::Resize(w, h) => {
                    renderer.resize(w, h);
                }
                _ => {}
            }
        }

        if input == GameInput::Quit {
            if quit_pending {
                return Ok(false);
            }
            quit_pending = true;
            game.notify("Quit? Press q again to confirm.".to_string());
            // Render the notification immediately
            game.draw(renderer);
            renderer.flush()?;
            continue;
        }
        if quit_pending && input != GameInput::None {
            // Any non-quit input cancels the quit
            quit_pending = false;
        }
        if input == GameInput::Restart {
            return Ok(true);
        }

        game.step(input, renderer)?;

        // Handle Load after step (replaces game state)
        if input == GameInput::Load
            && let Ok(loaded) = Game::load("savegame.json", game.target_fps)
        {
            *game = loaded;
        }

        // FPS counter
        frame_count += 1;
        let fps_elapsed = fps_timer.elapsed();
        if fps_elapsed >= Duration::from_secs(1) {
            display_fps = frame_count;
            frame_count = 0;
            fps_timer = Instant::now();
        }
        game.display_fps = Some(display_fps);

        // sleep to hit target fps — use sleep for bulk, spin-wait for precision
        let target = Duration::from_secs_f64(1.0 / game.target_fps as f64);
        let sleep_margin = Duration::from_millis(2);
        let elapsed = frame_start.elapsed();
        if let Some(remaining) = target.checked_sub(elapsed) {
            if remaining > sleep_margin {
                std::thread::sleep(remaining - sleep_margin);
            }
            // spin-wait the last ~2ms for precise timing
            while frame_start.elapsed() < target {
                std::hint::spin_loop();
            }
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--screenshot") {
        // Render a single frame as ANSI to stdout and exit
        let w: u16 = args
            .iter()
            .position(|a| a == "--width")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);
        let h: u16 = args
            .iter()
            .position(|a| a == "--height")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let ticks: u64 = args
            .iter()
            .position(|a| a == "--ticks")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        let seed: u32 = args
            .iter()
            .position(|a| a == "--seed")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);

        let mut r = headless_renderer::HeadlessRenderer::new(w, h);
        let mut game = Game::new(60, seed);
        game.half_speed_base = true; // interactive mode: deliberate pace at 1x
        if args.iter().any(|a| a == "--auto-build") {
            game.auto_build = true;
        }
        // Reveal entire map for screenshots (no fog of war)
        for y in 0..256 {
            for x in 0..256 {
                game.exploration.reveal(x, y, 0);
            }
        }
        for _ in 0..ticks {
            game.step(GameInput::None, &mut r)?;
            // Keep map fully revealed each tick
            for y in 0..256usize {
                for x in 0..256usize {
                    game.exploration.reveal(x, y, 0);
                }
            }
        }

        // Output as PNG if --png flag given, otherwise ANSI
        let png_path = args
            .iter()
            .position(|a| a == "--png")
            .and_then(|i| args.get(i + 1).cloned());

        // Print game state summary
        eprintln!(
            "=== State: tick={} season={} day={} hour={:.1} year={} ===",
            game.tick,
            game.day_night.season.name(),
            game.day_night.day + 1,
            game.day_night.hour,
            game.day_night.year + 1
        );
        eprintln!(
            "  resources: food={} wood={} stone={} planks={} masonry={}",
            game.resources.food,
            game.resources.wood,
            game.resources.stone,
            game.resources.planks,
            game.resources.masonry
        );
        {
            use terrain_gen_rust::ecs::{Creature, Species};
            let vc = game
                .world
                .query::<&Creature>()
                .iter()
                .filter(|c| c.species == Species::Villager)
                .count();
            let wc = game
                .world
                .query::<&Creature>()
                .iter()
                .filter(|c| c.species == Species::Predator)
                .count();
            eprintln!(
                "  pop: {} villagers, {} wolves, auto_build={}",
                vc, wc, game.auto_build
            );
        }
        eprintln!("  camera: ({}, {})", game.camera.x, game.camera.y);

        if let Some(path) = png_path {
            #[cfg(feature = "png")]
            {
                r.save_png(&path, 8, 16)?;
                eprintln!("Saved PNG: {}", path);
            }
            #[cfg(not(feature = "png"))]
            {
                eprintln!(
                    "PNG support requires --features png. Compile with: cargo run --release --features png"
                );
                let _ = path;
            }
        } else {
            // Emit ANSI-colored output
            for y in 0..h {
                for x in 0..w {
                    if let Some(cell) = r.get_cell(x, y) {
                        let fg = format!("\x1b[38;2;{};{};{}m", cell.fg.0, cell.fg.1, cell.fg.2);
                        let bg = match cell.bg {
                            Some(c) => format!("\x1b[48;2;{};{};{}m", c.0, c.1, c.2),
                            None => String::new(),
                        };
                        print!("{}{}{}", fg, bg, cell.ch);
                    }
                }
                println!("\x1b[0m");
            }
        }
        return Ok(());
    }

    if args.iter().any(|a| a == "--play") {
        // Non-interactive play mode: reads commands from --inputs or runs N ticks
        // Usage: --play [--width W] [--height H] [--seed S] [--inputs "tick:100,input:ScrollDown,tick:50"]
        // Or: --play --ticks 500 (just run and dump final frame)
        let w: u16 = args
            .iter()
            .position(|a| a == "--width")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(80);
        let h: u16 = args
            .iter()
            .position(|a| a == "--height")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        let seed: u32 = args
            .iter()
            .position(|a| a == "--seed")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);

        let diagnostics = args.iter().any(|a| a == "--diagnostics");
        let diagnostics_interval: u64 = args
            .iter()
            .position(|a| a == "--diagnostics-interval")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        let mut r = headless_renderer::HeadlessRenderer::new(w, h);
        let mut game_obj = Game::new(60, seed);
        if args.iter().any(|a| a == "--auto-build") {
            game_obj.auto_build = true;
        }

        let inputs_str = args
            .iter()
            .position(|a| a == "--inputs")
            .and_then(|i| args.get(i + 1).cloned())
            .unwrap_or_default();

        if diagnostics {
            // Diagnostics mode: run ticks, emit JSON lines, suppress frame output
            let ticks: u64 = args
                .iter()
                .position(|a| a == "--ticks")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse().ok())
                .unwrap_or(200);
            for _ in 0..ticks {
                game_obj.step(GameInput::None, &mut r)?;
                if game_obj.tick % diagnostics_interval == 0 {
                    println!("{}", game_obj.collect_diagnostics());
                }
            }
            println!("{}", game_obj.collect_diagnostics());
        } else if inputs_str.is_empty() {
            // Just run ticks and dump
            let ticks: u64 = args
                .iter()
                .position(|a| a == "--ticks")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse().ok())
                .unwrap_or(200);
            for _ in 0..ticks {
                game_obj.step(GameInput::None, &mut r)?;
            }
            // Dump frame as plain text (no ANSI) for easy reading
            print!("{}", r.frame_as_string());
        } else {
            // Parse commands: tick:N runs N ticks, then named inputs
            let mut last_cmd_was_frame = false;
            for cmd in inputs_str.split(',') {
                let cmd = cmd.trim();
                last_cmd_was_frame = false;
                if let Some(n) = cmd.strip_prefix("tick:") {
                    let ticks: u64 = n.parse().unwrap_or(1);
                    for _ in 0..ticks {
                        game_obj.step(GameInput::None, &mut r)?;
                    }
                } else if let Some(input_name) = cmd.strip_prefix("input:") {
                    let input = match input_name {
                        "ScrollUp" => GameInput::ScrollUp,
                        "ScrollDown" => GameInput::ScrollDown,
                        "ScrollLeft" => GameInput::ScrollLeft,
                        "ScrollRight" => GameInput::ScrollRight,
                        "TogglePause" => GameInput::TogglePause,
                        "ToggleBuildMode" => GameInput::ToggleBuildMode,
                        "BuildPlace" => GameInput::BuildPlace,
                        "BuildCycleType" => GameInput::BuildCycleType,
                        "BuildUp" => GameInput::BuildUp,
                        "BuildDown" => GameInput::BuildDown,
                        "BuildLeft" => GameInput::BuildLeft,
                        "BuildRight" => GameInput::BuildRight,
                        "Demolish" => GameInput::Demolish,
                        "CycleOverlay" => GameInput::CycleOverlay,
                        "CycleSpeed" => GameInput::CycleSpeed,
                        "GotoSettlement" => GameInput::GotoSettlement,
                        "ToggleAutoBuild" => GameInput::ToggleAutoBuild,
                        "ToggleRain" => GameInput::ToggleRain,
                        "Save" => GameInput::Save,
                        _ => GameInput::None,
                    };
                    game_obj.step(input, &mut r)?;
                } else if cmd.starts_with("seed:") {
                    // Seed must be passed via --seed CLI arg; this token is a no-op
                    // (kept for command readability)
                } else if cmd == "auto-build" {
                    // Directly enable auto-build (not a toggle, so safe to use at start)
                    game_obj.auto_build = true;
                } else if cmd == "frame" {
                    // Dump current frame
                    println!("{}", r.frame_as_string());
                    println!("--- tick {} ---", game_obj.tick);
                    last_cmd_was_frame = true;
                } else if cmd == "ansi" {
                    print!("{}", r.frame_as_ansi());
                    println!("--- tick {} ---", game_obj.tick);
                    last_cmd_was_frame = true;
                }
            }
            // Dump final frame only if the last command wasn't already a frame dump
            if !last_cmd_was_frame {
                println!("{}", r.frame_as_string());
            }
        }
        return Ok(());
    }

    if args.iter().any(|a| a == "--terrain") {
        // Terrain-only mode: just run water/erosion simulation, no entities
        // Usage: --terrain [--seed S] [--ticks N] [--png out.png]
        let seed: u32 = args
            .iter()
            .position(|a| a == "--seed")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);
        let w: u16 = args
            .iter()
            .position(|a| a == "--width")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(160);
        let h: u16 = args
            .iter()
            .position(|a| a == "--height")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(48);

        use terrain_gen_rust::terrain_gen::TerrainGenConfig;
        use terrain_gen_rust::terrain_pipeline::{PipelineConfig, run_pipeline};

        eprintln!("Running terrain pipeline on seed {}...", seed);
        let pipeline_config = PipelineConfig {
            terrain: TerrainGenConfig {
                seed,
                scale: 0.015,
                ..Default::default()
            },
            ..PipelineConfig::default()
        };
        let result = run_pipeline(256, 256, &pipeline_config);
        let map = result.map;
        let river_count = result.river_mask.iter().filter(|r| **r).count();
        eprintln!("  rivers: {} cells", river_count);
        let mut biome_counts = std::collections::HashMap::new();
        for y in 0..256 {
            for x in 0..256 {
                if let Some(t) = map.get(x, y) {
                    *biome_counts.entry(format!("{:?}", t)).or_insert(0u32) += 1;
                }
            }
        }
        eprintln!("  biomes: {:?}", biome_counts);

        // Render terrain to a headless renderer (no entities, no panel)
        let mut r = headless_renderer::HeadlessRenderer::new(w, h);
        for sy in 0..h {
            for sx in 0..w {
                let wx = sx as usize / 2; // CELL_ASPECT=2
                let wy = sy as usize;
                if let Some(terrain) = map.get(wx, wy) {
                    let fg = terrain.fg();
                    let bg = terrain.bg().unwrap_or(renderer::Color(0, 0, 0));
                    r.draw(sx, sy, terrain.ch(), fg, Some(bg));
                }
            }
        }

        let png_path = args
            .iter()
            .position(|a| a == "--png")
            .and_then(|i| args.get(i + 1).cloned());
        if let Some(path) = png_path {
            #[cfg(feature = "png")]
            {
                r.save_png(&path, 8, 16)?;
                eprintln!("Saved terrain PNG: {}", path);
            }
            #[cfg(not(feature = "png"))]
            {
                eprintln!("PNG requires --features png");
                let _ = path;
            }
        } else {
            print!("{}", r.frame_as_string());
        }
        return Ok(());
    }

    // --live-gen: show terrain generation in real time.
    // Renders the heightmap as grayscale while erosion runs, ~30fps.
    if args.iter().any(|a| a == "--live-gen") {
        let seed: u32 = args
            .iter()
            .position(|a| a == "--seed")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);
        let mut renderer = CrosstermRenderer::new()?;
        let (w, h) = renderer.size();
        let map_w = 256usize;
        let map_h = 256usize;

        // Generate normalized terrain (Nick's approach)
        let mut heights = terrain_gen_rust::terrain_pipeline::generate_normalized_terrain(
            map_w, map_h, seed,
        );

        // Set up hydrology
        let params = terrain_gen_rust::hydrology::HydroParams {
            sea_level: 0.0,
            ..Default::default()
        };
        let mut hydro = terrain_gen_rust::hydrology::HydroMap::new(map_w, map_h);

        let area = (map_w * map_h) as f64;
        let nick_area = 512.0 * 512.0;
        let particles = ((512.0 * area / nick_area).round() as u32).max(32);
        let target_total = 2_560_000.0 * area / nick_area;
        let total_cycles = ((target_total / particles as f64).round() as u32).max(50);
        let render_interval = (total_cycles / 100).max(1); // ~100 frames

        use terrain_gen_rust::renderer::{Color, Renderer};

        for cycle in 0..total_cycles {
            terrain_gen_rust::hydrology::erode(
                &mut heights, &mut hydro, &params,
                particles, seed.wrapping_add(cycle),
            );

            // Render every N cycles
            if cycle % render_interval == 0 || cycle == total_cycles - 1 {
                let pct = (cycle + 1) as f64 / total_cycles as f64 * 100.0;
                let min_h = heights.iter().cloned().fold(f64::INFINITY, f64::min);
                let max_h = heights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let range = (max_h - min_h).max(1e-10);

                // Render heightmap using half-blocks for 2x vertical density
                for sy in 0..(h.saturating_sub(2)) {
                    for sx in 0..w {
                        // Map screen coords to terrain coords
                        let mx = (sx as f64 / w as f64 * map_w as f64) as usize;
                        let my_top = (sy as f64 * 2.0 / (h as f64 * 2.0) * map_h as f64) as usize;
                        let my_bot = ((sy as f64 * 2.0 + 1.0) / (h as f64 * 2.0) * map_h as f64) as usize;

                        let to_gray = |mx: usize, my: usize| -> Color {
                            if mx >= map_w || my >= map_h {
                                return Color(0, 0, 0);
                            }
                            let i = my * map_w + mx;
                            let v = ((heights[i] - min_h) / range * 255.0) as u8;
                            // Discharge tint
                            let d = terrain_gen_rust::hydrology::erf_approx(
                                0.4 * hydro.discharge[i],
                            );
                            if d > 0.1 {
                                let a = d.min(0.9);
                                Color(
                                    (v as f64 * (1.0 - a)) as u8,
                                    (v as f64 * (1.0 - a) + 50.0 * a) as u8,
                                    (v as f64 * (1.0 - a) + 200.0 * a) as u8,
                                )
                            } else {
                                Color(v, v, v)
                            }
                        };

                        let top = to_gray(mx, my_top);
                        let bot = to_gray(mx, my_bot);
                        renderer.draw(sx, sy, '▄', bot, Some(top));
                    }
                }

                // Status line
                let status = format!(
                    " Erosion: {:.0}% ({}/{} cycles) particles={} min={:.3} max={:.3}",
                    pct, cycle + 1, total_cycles, particles, min_h, max_h
                );
                for (i, ch) in status.chars().enumerate() {
                    if i < w as usize {
                        renderer.draw(
                            i as u16,
                            h - 1,
                            ch,
                            Color(255, 220, 100),
                            Some(Color(25, 25, 40)),
                        );
                    }
                }
                renderer.flush()?;
            }
        }

        // Pass heights + hydro to the game instead of re-running the pipeline.
        // Drop the live-gen renderer first so showcase can create its own.
        drop(renderer);

        // Run the rest of the pipeline (biome classification, soil, etc.)
        // on the already-eroded heightmap.
        use terrain_gen_rust::terrain_pipeline::PipelineConfig;
        let mut pipeline_config = PipelineConfig::default();
        pipeline_config.terrain.seed = seed;
        // Skip erosion in the pipeline — we already did it
        pipeline_config.erosion_model = terrain_gen_rust::terrain_pipeline::ErosionModel::Off;

        let mut result = terrain_gen_rust::terrain_pipeline::run_pipeline(map_w, map_h, &pipeline_config);
        // Overwrite with our eroded heights and discharge
        result.heights = heights;
        result.discharge = hydro.discharge.clone();
        result.hydro = hydro;
        // Recompute water level from eroded heights
        let mut sorted_h = result.heights.clone();
        sorted_h.sort_by(|a, b| a.partial_cmp(b).unwrap());
        result.water_level = sorted_h[sorted_h.len() * 10 / 100];

        // Reclassify biomes from eroded heightmap
        let slope = terrain_gen_rust::terrain_pipeline::compute_slope(&result.heights, map_w, map_h);
        let temperature = terrain_gen_rust::terrain_pipeline::compute_temperature(&result.heights, map_w, map_h, seed);
        for y in 0..map_h {
            for x in 0..map_w {
                let i = y * map_w + x;
                let terrain = terrain_gen_rust::terrain_pipeline::classify_biome(
                    result.heights[i], temperature[i], result.moisture[i],
                    slope[i], result.water_level,
                );
                result.map.set(x, y, terrain);
            }
        }
        result.slope = slope;
        result.temperature = temperature;

        let mut renderer = CrosstermRenderer::new()?;
        let mut game = Game::new_from_pipeline(60, seed, result);

        game.half_speed_base = true;
        game.world.clear();
        let (mw, mh) = (game.map.width, game.map.height);
        for y in 0..mh {
            for x in 0..mw {
                game.exploration.reveal(x, y, 1);
            }
        }
        game.raining = false;
        game.render_mode = terrain_gen_rust::game::RenderMode::Landscape;
        run_interactive(&mut game, &mut renderer)?;
        return Ok(());
    }

    // --showcase: terrain-only mode. No entities, no fog of war, just
    // terrain + lighting + day/night + seasons + weather. For tuning visuals.
    if args.iter().any(|a| a == "--showcase") {
        let seed: u32 = args
            .iter()
            .position(|a| a == "--seed")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);
        let mut renderer = CrosstermRenderer::new()?;
        let mut game = Game::new(60, seed);
        game.half_speed_base = true;
        // Remove all entities (villagers, prey, predators, buildings, etc.)
        game.world.clear();
        // Reveal entire map (no fog of war)
        let (mw, mh) = (game.map.width, game.map.height);
        for y in 0..mh {
            for x in 0..mw {
                game.exploration.reveal(x, y, 1);
            }
        }
        // Start dry — player can toggle rain with 'r'
        game.raining = false;
        // Start in Landscape mode
        game.render_mode = terrain_gen_rust::game::RenderMode::Landscape;
        run_interactive(&mut game, &mut renderer)?;
        return Ok(());
    }

    let mut renderer = CrosstermRenderer::new()?;
    let mut seed = 42u32;
    loop {
        let mut game = Game::new(60, seed);
        game.half_speed_base = true; // interactive mode: deliberate pace at 1x
        let restart = run_interactive(&mut game, &mut renderer)?;
        if !restart {
            break;
        }
        seed = seed.wrapping_add(1); // new seed each restart for variety
    }
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
        let non_blank = snap
            .text
            .chars()
            .filter(|c| *c != ' ' && *c != '\n')
            .count();
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
        assert!(
            has_terrain,
            "frame should contain terrain characters:\n{}",
            frame
        );
    }

    #[test]
    fn game_draws_status_line() {
        let mut r = HeadlessRenderer::new(120, 20);
        let mut game = test_game();
        game.step(GameInput::None, &mut r).unwrap();

        let frame = r.frame_as_string();
        assert!(
            frame.contains("tick:1"),
            "expected tick in status line:\n{}",
            frame
        );
    }

    #[test]
    fn entities_move_between_ticks() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = test_game();

        let snap1 = game.step_headless(GameInput::None, &mut r).unwrap();
        let snap2 = game.step_headless(GameInput::None, &mut r).unwrap();

        let diff = snap1.diff(&snap2);
        assert!(
            !diff.changes.is_empty(),
            "NPC movement should cause frame changes"
        );
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
        assert!(
            diff.changes.is_empty(),
            "diffing a frame against itself should be empty"
        );
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
            if has_water {
                break;
            }
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
        assert!(
            frame.contains("rain:[r]-"),
            "should show rain off:\n{}",
            frame
        );

        game.step(GameInput::ToggleRain, &mut r).unwrap();
        let frame = r.frame_as_string();
        assert!(
            frame.contains("rain:[r]+"),
            "should show rain ON:\n{}",
            frame
        );
    }

    #[test]
    fn toggle_day_night() {
        let mut r = HeadlessRenderer::new(100, 20);
        let mut game = test_game();
        assert!(game.day_night.enabled);

        game.step(GameInput::None, &mut r).unwrap();
        let frame = r.frame_as_string();
        assert!(
            frame.contains("time:[t]+"),
            "should show time ON:\n{}",
            frame
        );

        game.step(GameInput::ToggleDayNight, &mut r).unwrap();
        assert!(!game.day_night.enabled);
        let frame = r.frame_as_string();
        assert!(
            frame.contains("time:[t]-"),
            "should show time off:\n{}",
            frame
        );
    }

    #[test]
    fn toggle_render_mode_cycle() {
        use terrain_gen_rust::game::RenderMode;
        let mut r = HeadlessRenderer::new(140, 20);
        let mut game = test_game();
        assert_eq!(game.render_mode, RenderMode::Normal);

        game.step(GameInput::None, &mut r).unwrap();
        let frame = r.frame_as_string();
        assert!(
            frame.contains("view:[v]-"),
            "should show normal view:\n{}",
            frame
        );

        // First toggle: Normal -> Map
        game.step(GameInput::ToggleDebugView, &mut r).unwrap();
        assert_eq!(game.render_mode, RenderMode::Map);
        let frame = r.frame_as_string();
        assert!(
            frame.contains("view:[v]M"),
            "should show Map view:\n{}",
            frame
        );

        // Second toggle: Map -> Debug
        game.step(GameInput::ToggleDebugView, &mut r).unwrap();
        assert_eq!(game.render_mode, RenderMode::Debug);
        let frame = r.frame_as_string();
        assert!(
            frame.contains("view:[v]D"),
            "should show DEBUG view:\n{}",
            frame
        );

        // debug view uses uppercase terrain letters
        let has_debug_chars = frame.chars().any(|c| "WSGFMN".contains(c));
        assert!(
            has_debug_chars,
            "debug view should use uppercase terrain letters:\n{}",
            frame
        );

        // Third toggle: Debug -> Normal
        game.step(GameInput::ToggleDebugView, &mut r).unwrap();
        assert_eq!(game.render_mode, RenderMode::Normal);
    }

    #[test]
    fn day_night_affects_colors() {
        let mut r = HeadlessRenderer::new(60, 20);
        let mut game = test_game();

        // Noon: bright
        game.day_night.hour = 12.0;
        game.step(GameInput::None, &mut r).unwrap();
        let noon_snap = game.step_headless(GameInput::None, &mut r).unwrap();

        // Midnight: dark
        game.day_night.hour = 0.0;
        let midnight_snap = game.step_headless(GameInput::None, &mut r).unwrap();

        // Compare brightness of terrain cells in the map area (past the panel)
        // Average brightness of terrain cells (non-panel bg)
        let panel_bg = renderer::Color(25, 25, 40);
        let sample_brightness = |snap: &game::FrameSnapshot| -> u32 {
            let mut total = 0u64;
            let mut count = 0u64;
            for y in 0..snap.cells.len() {
                for x in 24..snap.cells.first().map_or(0, |r| r.len()) {
                    if x < snap.cells[y].len() {
                        let c = &snap.cells[y][x];
                        if let Some(bg) = c.bg {
                            // Skip panel cells and fog (very dark)
                            if bg != panel_bg && (bg.0 as u32 + bg.1 as u32 + bg.2 as u32) > 30 {
                                total += c.fg.0 as u64 + c.fg.1 as u64 + c.fg.2 as u64;
                                count += 1;
                            }
                        }
                    }
                }
            }
            if count == 0 {
                0
            } else {
                (total / count) as u32
            }
        };
        let noon_brightness = sample_brightness(&noon_snap);
        let midnight_brightness = sample_brightness(&midnight_snap);

        assert!(
            noon_brightness > midnight_brightness,
            "noon should be brighter than midnight: noon={} midnight={}",
            noon_brightness,
            midnight_brightness
        );
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

    #[test]
    fn profile_frame_phases() {
        use crate::renderer::Renderer;
        use std::time::Instant;

        let mut r = HeadlessRenderer::new(120, 40); // realistic terminal size
        let mut game = test_game();

        // Warm up
        for _ in 0..10 {
            game.step(GameInput::None, &mut r).unwrap();
        }

        let hours_to_test = [12.0, 6.5, 18.5, 0.0]; // noon, sunrise, sunset, midnight
        let labels = ["noon", "sunrise", "sunset", "midnight"];

        for (hour, label) in hours_to_test.iter().zip(labels.iter()) {
            game.day_night.hour = *hour;
            game.raining = true; // worst case: water sim active

            let n = 100;
            let start = Instant::now();
            for _ in 0..n {
                game.step(GameInput::None, &mut r).unwrap();
            }
            let total = start.elapsed();
            let per_frame_us = total.as_micros() / n as u128;
            let fps = 1_000_000.0 / per_frame_us as f64;

            eprintln!("  {}: {:.0}us/frame ({:.0} fps)", label, per_frame_us, fps);
        }

        // Now profile individual phases at sunset (worst case)
        game.day_night.hour = 18.5;
        game.raining = true;

        let n = 100;

        // Phase: water + moisture (pipe_water is the sole water sim now)
        let start = Instant::now();
        for _ in 0..n {
            game.pipe_water.step(&game.heights, 0.1);
            game.moisture.update(
                &mut game.pipe_water,
                &mut game.vegetation,
                &game.map,
                &mut game.wind,
                &game.heights,
            );
        }
        let water_us = start.elapsed().as_micros() / n as u128;

        // Phase: lighting
        let start = Instant::now();
        for _ in 0..n {
            game.day_night.compute_lighting(
                &game.heights,
                game.map.width,
                game.map.height,
                game.camera.x,
                game.camera.y,
                120,
                40,
            );
        }
        let light_us = start.elapsed().as_micros() / n as u128;

        // Phase: render
        let start = Instant::now();
        for _ in 0..n {
            r.clear();
            game.draw(&mut r);
        }
        let draw_us = start.elapsed().as_micros() / n as u128;

        // Phase: flush
        let start = Instant::now();
        for _ in 0..n {
            r.flush().unwrap();
        }
        let flush_us = start.elapsed().as_micros() / n as u128;

        eprintln!("\n  Sunset breakdown (120x40):");
        eprintln!("    water+moisture: {}us", water_us);
        eprintln!("    lighting:       {}us", light_us);
        eprintln!("    draw:           {}us", draw_us);
        eprintln!("    flush:          {}us", flush_us);
        eprintln!(
            "    total:          {}us ({:.0} fps budget)",
            water_us + light_us + draw_us + flush_us,
            1_000_000.0 / (water_us + light_us + draw_us + flush_us) as f64
        );
    }
}
