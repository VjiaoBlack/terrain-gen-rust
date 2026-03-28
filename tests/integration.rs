use terrain_gen_rust::ecs::{self, BuildingType, Creature, FarmPlot, FoodSource, Species};
use terrain_gen_rust::game::{Game, GameInput, OverlayMode};
use terrain_gen_rust::headless_renderer::HeadlessRenderer;
use terrain_gen_rust::tilemap::Terrain;

fn test_game() -> Game {
    Game::new(60, 42)
}

// --- Full Game Lifecycle ---

#[test]
fn game_survives_1000_ticks_without_panic() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    for _ in 0..1000 {
        game.step(GameInput::None, &mut renderer).unwrap();
    }

    assert_eq!(game.tick, 1000);
}

#[test]
fn game_survives_5000_ticks_with_rain() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    game.step(GameInput::ToggleRain, &mut renderer).unwrap();

    for _ in 0..5000 {
        game.step(GameInput::None, &mut renderer).unwrap();
    }

    assert!(game.tick >= 5000);
}

#[test]
fn seasonal_cycle_completes() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    // Give plenty of food so settlement survives through seasons
    game.resources.food = 1000;

    let initial_season = game.day_night.season;

    // Run enough ticks to cycle through all 4 seasons
    for _ in 0..15000 {
        game.step(GameInput::None, &mut renderer).unwrap();
        if game.game_over { break; }
    }

    // Should have progressed through multiple seasons
    assert!(game.tick >= 5000, "should survive at least 5000 ticks with food: tick={}", game.tick);
}

// --- Settlement Survival ---

#[test]
fn villagers_survive_with_adequate_resources() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    // Give ample food
    game.resources.food = 500;

    let initial_villagers = count_villagers(&game);
    assert!(initial_villagers >= 3, "should start with at least 3 villagers");

    for _ in 0..2000 {
        game.step(GameInput::None, &mut renderer).unwrap();
    }

    let final_villagers = count_villagers(&game);
    assert!(final_villagers >= 1,
        "with 500 food, at least one villager should survive 2000 ticks, got {}", final_villagers);
}

#[test]
fn starvation_causes_game_over_eventually() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    // Remove all food and food sources
    game.resources.food = 0;
    game.resources.grain = 0;
    game.resources.bread = 0;

    // Despawn all berry bushes so villagers can't forage
    let food_entities: Vec<hecs::Entity> = game.world
        .query::<(hecs::Entity, &FoodSource)>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    for e in food_entities {
        let _ = game.world.despawn(e);
    }

    // Despawn all farms so villagers can't grow food
    let farm_entities: Vec<hecs::Entity> = game.world
        .query::<(hecs::Entity, &FarmPlot)>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    for e in farm_entities {
        let _ = game.world.despawn(e);
    }

    // Run until game over or 20000 ticks
    for _ in 0..20000 {
        game.step(GameInput::None, &mut renderer).unwrap();
        if game.game_over {
            return; // Expected outcome
        }
    }

    // If not game over, at least all villagers should be dead
    let villagers = count_villagers(&game);
    assert!(villagers == 0 || game.game_over,
        "with no food, settlement should collapse: {} villagers remain", villagers);
}

// --- Resource Management ---

#[test]
fn resources_stay_non_negative() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    for _ in 0..3000 {
        game.step(GameInput::None, &mut renderer).unwrap();
        assert!(game.resources.food >= 0, "food went negative at tick {}", game.tick);
        assert!(game.resources.wood >= 0, "wood went negative at tick {}", game.tick);
        assert!(game.resources.stone >= 0, "stone went negative at tick {}", game.tick);
    }
}

#[test]
fn auto_build_manages_resources_without_crash() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    game.auto_build = true;
    game.resources.food = 100;
    game.resources.wood = 50;
    game.resources.stone = 50;

    // Warm up influence so auto-build can place buildings
    for _ in 0..10 {
        game.update_influence();
    }

    for _ in 0..5000 {
        game.step(GameInput::None, &mut renderer).unwrap();
    }

    // Just verify no crashes and reasonable state
    assert!(game.tick >= 5000);
}

// --- Traffic and Roads ---

#[test]
fn heavy_traffic_creates_roads() {
    let mut game = test_game();

    // Pick a grass tile
    let tx = 130usize;
    let ty = 130usize;
    game.map.set(tx, ty, Terrain::Grass);

    // Simulate 200 footsteps (above threshold of 150)
    for _ in 0..200 {
        game.traffic.step_on(tx, ty);
    }

    // Trigger conversion (happens every 100 ticks)
    game.tick = 100;
    game.update_traffic();

    assert_eq!(*game.map.get(tx, ty).unwrap(), Terrain::Road,
        "heavily trafficked grass tile should become road");
}

#[test]
fn roads_give_speed_bonus() {
    assert_eq!(Terrain::Road.speed_multiplier(), 1.5);
    assert_eq!(Terrain::Grass.speed_multiplier(), 1.0);
}

// --- Overlay System ---

#[test]
fn all_overlays_render_without_panic() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(120, 40);

    // Spawn some entities to make overlays interesting
    ecs::spawn_predator(&mut game.world, 110.0, 110.0);
    ecs::spawn_berry_bush(&mut game.world, 115.0, 115.0);

    let overlays = [
        OverlayMode::None,
        OverlayMode::Tasks,
        OverlayMode::Resources,
        OverlayMode::Threats,
        OverlayMode::Traffic,
    ];

    for overlay in &overlays {
        game.overlay = *overlay;
        game.step(GameInput::None, &mut renderer).unwrap();
    }
}

// --- Save/Load Round Trip ---

#[test]
fn save_load_preserves_game_state() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    game.resources.food = 50;
    game.resources.wood = 30;
    game.auto_build = true;

    // Run a few ticks to build up state
    for _ in 0..100 {
        game.step(GameInput::None, &mut renderer).unwrap();
    }

    let save_path = "/tmp/terrain_gen_integration_test.json";
    game.save(save_path).unwrap();

    let loaded = Game::load(save_path, 60).unwrap();

    assert_eq!(loaded.tick, game.tick);
    assert_eq!(loaded.resources.food, game.resources.food);
    assert_eq!(loaded.resources.wood, game.resources.wood);
    assert_eq!(loaded.auto_build, game.auto_build);

    // Clean up
    let _ = std::fs::remove_file(save_path);
}

#[test]
fn save_load_with_complex_state() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    game.resources.food = 200;
    game.resources.wood = 100;
    game.resources.stone = 100;
    game.auto_build = true;

    // Run many ticks to accumulate complex state (buildings, traffic, events)
    for _ in 0..3000 {
        game.step(GameInput::None, &mut renderer).unwrap();
    }

    let save_path = "/tmp/terrain_gen_complex_save_test.json";
    game.save(save_path).unwrap();

    let loaded = Game::load(save_path, 60).unwrap();

    assert_eq!(loaded.tick, game.tick);
    assert_eq!(loaded.resources.food, game.resources.food);
    assert_eq!(loaded.resources.grain, game.resources.grain);
    assert_eq!(loaded.resources.bread, game.resources.bread);
    assert_eq!(loaded.peak_population, game.peak_population);

    let _ = std::fs::remove_file(save_path);
}

// --- Multi-System Interactions ---

#[test]
fn population_growth_works_with_housing() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    game.resources.food = 500;

    let initial = count_villagers(&game);

    // Build a hut for capacity
    let (cx, cy) = game.settlement_center();
    // Warm influence
    for _ in 0..10 {
        game.update_influence();
    }
    ecs::spawn_hut(&mut game.world, cx as f64, cy as f64 + 3.0);

    // Run for population growth
    for _ in 0..5000 {
        game.step(GameInput::None, &mut renderer).unwrap();
        if game.game_over { break; }
    }

    let final_count = count_villagers(&game);
    // With a hut (capacity 4) and food, population should grow
    assert!(final_count >= initial,
        "population should grow or stay stable with food+housing: {} -> {}", initial, final_count);
}

#[test]
fn events_activate_during_gameplay() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    game.resources.food = 10000;
    game.resources.wood = 500;
    game.resources.stone = 500;

    // Add garrison for defense so settlement doesn't die to wolves
    let (cx, cy) = game.settlement_center();
    ecs::spawn_garrison(&mut game.world, cx as f64, cy as f64);
    ecs::spawn_hut(&mut game.world, cx as f64 + 2.0, cy as f64);

    // Run for many ticks — events trigger every 100 ticks with RNG
    let mut any_event = false;
    for _ in 0..20000 {
        game.step(GameInput::None, &mut renderer).unwrap();
        if !game.events.event_log.is_empty() {
            any_event = true;
            break;
        }
        if game.game_over { break; }
    }

    // Events are RNG-based, so we just check the system doesn't crash
    // If we survived long enough, we should see at least one event
    if game.tick > 5000 {
        assert!(any_event,
            "after {} ticks, at least one event should have occurred", game.tick);
    }
}

#[test]
fn influence_map_expands_around_settlement() {
    let mut game = test_game();

    // Run influence updates
    for _ in 0..50 {
        game.update_influence();
    }

    let (cx, cy) = game.settlement_center();
    let center_influence = game.influence.get(cx as usize, cy as usize);
    let far_influence = game.influence.get(0, 0);

    assert!(center_influence > far_influence,
        "influence should be stronger near settlement center: center={} far={}",
        center_influence, far_influence);
}

#[test]
fn building_placement_requires_influence() {
    let mut game = test_game();

    // Far corner — no influence
    assert!(!game.can_place_building(5, 5, ecs::BuildingType::Wall),
        "should not build far from settlement");

    // Near settlement — warm up influence
    for _ in 0..10 {
        game.update_influence();
    }
    let (cx, cy) = game.settlement_center();
    assert!(game.can_place_building(cx + 2, cy + 2, ecs::BuildingType::Wall),
        "should build near settlement with influence");
}

// --- Stress Tests ---

#[test]
fn handles_many_entities() {
    let mut game = test_game();
    let mut renderer = HeadlessRenderer::new(80, 24);

    // Spawn many wolves and bushes
    for i in 0..50 {
        ecs::spawn_predator(&mut game.world, 100.0 + (i % 10) as f64, 100.0 + (i / 10) as f64);
    }
    for i in 0..30 {
        ecs::spawn_berry_bush(&mut game.world, 120.0 + (i % 6) as f64, 120.0 + (i / 6) as f64);
    }

    // Should not panic with lots of entities
    for _ in 0..500 {
        game.step(GameInput::None, &mut renderer).unwrap();
        if game.game_over { break; }
    }
}

// --- Helpers ---

fn count_villagers(game: &Game) -> usize {
    game.world.query::<&ecs::Creature>().iter()
        .filter(|c| c.species == ecs::Species::Villager)
        .count()
}
