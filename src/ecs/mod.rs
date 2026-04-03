mod ai;
pub mod components;
pub mod serialize;
pub mod spatial;
pub mod spawn;
pub mod systems;

// Re-export everything so existing code using `crate::ecs::*` still works
pub use components::*;
pub use serialize::*;
pub use spawn::*;
pub use systems::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::spatial::SpatialHashGrid;
    use crate::headless_renderer::HeadlessRenderer;
    use crate::renderer::Color;
    use crate::simulation::{MoistureMap, Season};
    use crate::tilemap::{Terrain, TileMap};
    use hecs::World;

    fn walkable_map(w: usize, h: usize) -> TileMap {
        TileMap::new(w, h, Terrain::Grass)
    }

    /// Build and populate a spatial grid from the current world state.
    fn make_grid(world: &World, map: &TileMap) -> SpatialHashGrid {
        let mut grid = SpatialHashGrid::new(map.width, map.height, 16);
        grid.populate(world);
        grid
    }

    /// Create a MoistureMap with uniform high moisture (0.6) so moisture_ramp returns 1.0.
    /// This preserves existing test behavior where growth rate is unscaled.
    fn wet_moisture_map() -> MoistureMap {
        let mut mm = MoistureMap::new(64, 64);
        // Set all tiles to 0.6 so ramp returns 1.0
        for y in 0..64 {
            for x in 0..64 {
                mm.set(x, y, 0.6);
            }
        }
        mm
    }

    #[test]
    fn spawn_and_query() {
        let mut world = World::new();
        spawn_entity(&mut world, 5.0, 3.0, 0.0, 0.0, '@', Color(255, 255, 255));

        let mut count = 0;
        for (pos, sprite) in world.query::<(&Position, &Sprite)>().iter() {
            assert_eq!(pos.x, 5.0);
            assert_eq!(pos.y, 3.0);
            assert_eq!(sprite.ch, '@');
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn movement_system_updates_position() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_entity(&mut world, 10.0, 5.0, 1.5, -0.5, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 11.5);
        assert_eq!(pos.y, 4.5);
    }

    #[test]
    fn movement_accumulates_over_ticks() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_entity(&mut world, 0.0, 0.0, 1.0, 1.0, '@', Color(255, 255, 255));

        for _ in 0..10 {
            system_movement(&mut world, &map);
        }

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 10.0);
    }

    #[test]
    fn collision_blocks_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        for y in 0..10 {
            map.set(5, y, Terrain::BuildingWall);
        }
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "should be blocked by building wall");
    }

    #[test]
    fn collision_bounces_velocity() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::BuildingWall);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let vel = world.get::<&Velocity>(e).unwrap();
        assert_eq!(vel.dx, -1.0, "velocity should bounce on collision");
    }

    #[test]
    fn slides_along_wall() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        for y in 0..10 {
            map.set(5, y, Terrain::BuildingWall);
        }
        let e = spawn_entity(&mut world, 4.0, 4.0, 1.0, 1.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "x should be blocked");
        assert_eq!(pos.y, 5.0, "y should still move (slide)");
    }

    #[test]
    fn water_slows_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::Water);
        // Start ON the water tile — speed multiplier applies to current tile
        let e = spawn_entity(&mut world, 5.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        // Water is swimmable but very slow (0.15x), so 5.0 + 1.0*0.15 = 5.15
        assert!(pos.x > 5.0, "should move in water (slowly)");
        assert!(pos.x < 5.5, "should be very slow in water");
    }

    #[test]
    fn mountain_slows_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::Mountain);
        // Start ON the mountain tile — speed multiplier applies to current tile
        let e = spawn_entity(&mut world, 5.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        // Mountain is walkable but slow (0.25x), so 5.0 + 1.0*0.25 = 5.25
        assert!(pos.x > 5.0, "should move on mountain (slowly)");
        assert!(pos.x < 6.0, "should be slowed by mountain");
    }

    #[test]
    fn forest_slower_than_grass() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(3, 5, Terrain::Forest);
        // Entity on forest
        let e_forest = spawn_entity(&mut world, 3.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));
        // Entity on grass
        let e_grass = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pf = world.get::<&Position>(e_forest).unwrap().x;
        let pg = world.get::<&Position>(e_grass).unwrap().x;
        assert!(
            pf < pg,
            "forest entity should move slower than grass entity"
        );
    }

    #[test]
    fn render_system_draws_sprites() {
        let mut world = World::new();
        spawn_entity(&mut world, 3.0, 2.0, 0.0, 0.0, '@', Color(0, 255, 0));

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let cell = r.get_cell(3, 2).unwrap();
        assert_eq!(cell.ch, '@');
        assert_eq!(cell.fg, Color(0, 255, 0));
    }

    #[test]
    fn render_skips_negative_positions() {
        let mut world = World::new();
        spawn_entity(&mut world, -5.0, -3.0, 0.0, 0.0, 'X', Color(255, 0, 0));

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let frame = r.frame_as_string();
        assert!(!frame.contains('X'));
    }

    #[test]
    fn render_skips_out_of_bounds() {
        let mut world = World::new();
        spawn_entity(&mut world, 100.0, 100.0, 0.0, 0.0, 'X', Color(255, 0, 0));

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let frame = r.frame_as_string();
        assert!(!frame.contains('X'));
    }

    #[test]
    fn multiple_entities_render() {
        let mut world = World::new();
        spawn_entity(&mut world, 1.0, 0.0, 0.0, 0.0, 'A', Color(255, 0, 0));
        spawn_entity(&mut world, 3.0, 0.0, 0.0, 0.0, 'B', Color(0, 255, 0));
        spawn_entity(&mut world, 5.0, 0.0, 0.0, 0.0, 'C', Color(0, 0, 255));

        let mut r = HeadlessRenderer::new(10, 3);
        system_render(&world, &mut r);

        assert_eq!(r.get_cell(1, 0).unwrap().ch, 'A');
        assert_eq!(r.get_cell(3, 0).unwrap().ch, 'B');
        assert_eq!(r.get_cell(5, 0).unwrap().ch, 'C');
    }

    #[test]
    fn despawn_removes_entity() {
        let mut world = World::new();
        let e = spawn_entity(&mut world, 5.0, 3.0, 0.0, 0.0, '@', Color(255, 255, 255));
        world.despawn(e).unwrap();

        let mut r = HeadlessRenderer::new(10, 5);
        system_render(&world, &mut r);

        let frame = r.frame_as_string();
        assert!(!frame.contains('@'));
    }

    #[test]
    fn npc_wanders_and_moves() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let e = spawn_npc(&mut world, 15.0, 15.0, 0.2, '☺', Color(200, 100, 50));

        let start_pos = *world.get::<&Position>(e).unwrap();

        for _ in 0..500 {
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);
        }

        let end_pos = *world.get::<&Position>(e).unwrap();
        let dist = ((end_pos.x - start_pos.x).powi(2) + (end_pos.y - start_pos.y).powi(2)).sqrt();
        assert!(
            dist > 0.1,
            "NPC should have moved from spawn: dist={}",
            dist
        );
    }

    #[test]
    fn npc_stays_on_walkable_terrain() {
        let mut world = World::new();
        let mut map = TileMap::new(20, 20, Terrain::Water);
        for y in 5..15 {
            for x in 5..15 {
                map.set(x, y, Terrain::Grass);
            }
        }
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.3, '☺', Color(200, 100, 50));

        for _ in 0..500 {
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);
        }

        let pos = *world.get::<&Position>(e).unwrap();
        assert!(
            map.is_walkable(pos.x, pos.y),
            "NPC should stay on walkable terrain: pos=({}, {})",
            pos.x,
            pos.y
        );
    }

    #[test]
    fn idle_npc_stays_still() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.2, '☺', Color(200, 100, 50));

        {
            let mut behavior = world.get::<&mut Behavior>(e).unwrap();
            behavior.state = BehaviorState::Idle { timer: 100 };
        }

        let start_pos = *world.get::<&Position>(e).unwrap();

        for _ in 0..50 {
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);
        }

        let end_pos = *world.get::<&Position>(e).unwrap();
        assert_eq!(start_pos.x, end_pos.x, "idle NPC should not move");
        assert_eq!(start_pos.y, end_pos.y, "idle NPC should not move");
    }

    #[test]
    fn seek_moves_toward_target() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_npc(&mut world, 5.0, 5.0, 0.3, '☺', Color(200, 100, 50));

        {
            let mut behavior = world.get::<&mut Behavior>(e).unwrap();
            behavior.state = BehaviorState::Seek {
                target_x: 15.0,
                target_y: 15.0,
                reason: SeekReason::Unknown,
            };
        }

        let mut min_dist = f64::INFINITY;
        for _ in 0..200 {
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);
            let pos = *world.get::<&Position>(e).unwrap();
            let dist = ((pos.x - 15.0).powi(2) + (pos.y - 15.0).powi(2)).sqrt();
            if dist < min_dist {
                min_dist = dist;
            }
        }

        assert!(
            min_dist < 2.0,
            "NPC should reach near target: min_dist={}",
            min_dist
        );
    }

    #[test]
    fn hunger_increases_over_time() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        let start_hunger = world.get::<&Creature>(e).unwrap().hunger;

        for _ in 0..100 {
            system_hunger(&mut world, 1.0);
        }

        let end_hunger = world.get::<&Creature>(e).unwrap().hunger;
        assert!(
            end_hunger > start_hunger,
            "hunger should increase: {} -> {}",
            start_hunger,
            end_hunger
        );
        let expected = 0.0005 * 100.0;
        assert!(
            (end_hunger - start_hunger - expected).abs() < 0.001,
            "hunger should increase by 0.0005/tick for prey"
        );
    }

    #[test]
    fn prey_seeks_food_when_hungry() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        let _bush = spawn_berry_bush(&mut world, 20.0, 10.0);

        {
            let mut c = world.get::<&mut Creature>(prey).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(prey).unwrap().state;
        match state {
            BehaviorState::Seek {
                target_x, target_y, ..
            } => {
                assert!((target_x - 20.0).abs() < 0.1, "should seek food x");
                assert!((target_y - 10.0).abs() < 0.1, "should seek food y");
            }
            _ => panic!("hungry prey should seek food, got: {:?}", state),
        }
    }

    #[test]
    fn prey_flees_when_predator_nearby() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 5.0, 5.0);
        let _predator = spawn_predator(&mut world, 12.0, 10.0);

        {
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(prey).unwrap().state;
        assert!(
            matches!(state, BehaviorState::FleeHome { .. }),
            "prey should flee when predator nearby, got: {:?}",
            state
        );
    }

    #[test]
    fn prey_reaches_home_and_rests() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);

        {
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::FleeHome { timer: 120 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(prey).unwrap().state;
        assert!(
            matches!(state, BehaviorState::AtHome { .. }),
            "prey at home position should transition to AtHome, got: {:?}",
            state
        );
    }

    #[test]
    fn predator_hunts_visible_prey() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let predator = spawn_predator(&mut world, 10.0, 10.0);
        let _prey = spawn_prey(&mut world, 15.0, 10.0, 25.0, 25.0);

        {
            let mut c = world.get::<&mut Creature>(predator).unwrap();
            c.hunger = 0.5;
            let mut b = world.get::<&mut Behavior>(_prey).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(predator).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Hunting { .. }),
            "hungry predator should hunt visible prey, got: {:?}",
            state
        );
    }

    #[test]
    fn predator_ignores_prey_at_home() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let predator = spawn_predator(&mut world, 10.0, 10.0);
        let prey = spawn_prey(&mut world, 12.0, 10.0, 12.0, 10.0);

        {
            let mut c = world.get::<&mut Creature>(predator).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::AtHome { timer: 100 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(predator).unwrap().state;
        assert!(
            !matches!(state, BehaviorState::Hunting { .. }),
            "predator should not hunt prey that is at home, got: {:?}",
            state
        );
    }

    #[test]
    fn wolf_hunts_and_kills_rabbit() {
        let mut world = World::new();
        let map = walkable_map(50, 50);
        let wolf = spawn_predator(&mut world, 10.0, 10.0);
        let rabbit = spawn_prey(&mut world, 15.0, 10.0, 40.0, 40.0);

        {
            let mut c = world.get::<&mut Creature>(wolf).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(rabbit).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let mut wolf_ate = false;
        let mut rabbit_alive = true;

        for tick in 0..300 {
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);

            let wolf_state = world.get::<&Behavior>(wolf).unwrap().state;
            let rabbit_exists = world.get::<&Position>(rabbit).is_ok();

            if matches!(wolf_state, BehaviorState::Eating { .. }) {
                wolf_ate = true;
            }
            if !rabbit_exists {
                rabbit_alive = false;
                eprintln!(
                    "tick {}: rabbit despawned, wolf state: {:?}",
                    tick, wolf_state
                );
                break;
            }

            if tick < 5 || tick % 20 == 0 {
                let wp = *world.get::<&Position>(wolf).unwrap();
                let rp = *world.get::<&Position>(rabbit).unwrap();
                let rs = world.get::<&Behavior>(rabbit).unwrap().state;
                let d = ((wp.x - rp.x).powi(2) + (wp.y - rp.y).powi(2)).sqrt();
                eprintln!(
                    "tick {}: wolf({:.1},{:.1}) {:?}  rabbit({:.1},{:.1}) {:?}  dist={:.1}",
                    tick, wp.x, wp.y, wolf_state, rp.x, rp.y, rs, d
                );
            }
        }

        assert!(wolf_ate, "wolf should have entered Eating state");
        assert!(!rabbit_alive, "rabbit should have been killed");
    }

    #[test]
    fn full_ecosystem_simulation() {
        let mut world = World::new();
        let map = walkable_map(60, 60);

        spawn_den(&mut world, 10.0, 10.0);
        let rabbit = spawn_prey(&mut world, 11.0, 11.0, 10.0, 10.0);
        spawn_berry_bush(&mut world, 30.0, 30.0);
        let wolf = spawn_predator(&mut world, 25.0, 25.0);

        {
            let mut c = world.get::<&mut Creature>(wolf).unwrap();
            c.hunger = 0.6;
        }

        let mut states_seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for tick in 0..1000 {
            system_hunger(&mut world, 1.0);
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);

            let ws = world.get::<&Behavior>(wolf).unwrap().state;
            let state_name = format!("{:?}", ws)
                .split('{')
                .next()
                .unwrap_or("?")
                .split('(')
                .next()
                .unwrap_or("?")
                .trim()
                .to_string();
            states_seen.insert(format!("wolf:{}", state_name));

            if let Ok(rb) = world.get::<&Behavior>(rabbit) {
                let rstate = format!("{:?}", rb.state)
                    .split('{')
                    .next()
                    .unwrap_or("?")
                    .split('(')
                    .next()
                    .unwrap_or("?")
                    .trim()
                    .to_string();
                states_seen.insert(format!("rabbit:{}", rstate));
            }

            if tick % 100 == 0 {
                let wh = world.get::<&Creature>(wolf).unwrap().hunger;
                let rabbit_alive = world.get::<&Position>(rabbit).is_ok();
                eprintln!(
                    "tick {}: wolf hunger={:.2} state={:?} rabbit_alive={}",
                    tick, wh, ws, rabbit_alive
                );
            }
        }

        eprintln!("all states seen: {:?}", states_seen);
    }

    #[test]
    fn eating_reduces_hunger() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let prey = spawn_prey(&mut world, 10.0, 10.0, 5.0, 5.0);
        let _bush = spawn_berry_bush(&mut world, 10.0, 10.0);

        {
            let mut c = world.get::<&mut Creature>(prey).unwrap();
            c.hunger = 0.6;
            let mut b = world.get::<&mut Behavior>(prey).unwrap();
            b.state = BehaviorState::Eating { timer: 30 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let hunger = world.get::<&Creature>(prey).unwrap().hunger;
        assert!(hunger < 0.6, "eating should reduce hunger: {}", hunger);
    }

    #[test]
    fn villager_seeks_food_when_hungry() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        let _bush = spawn_berry_bush(&mut world, 20.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.8;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        match state {
            BehaviorState::Seek {
                target_x, target_y, ..
            } => {
                assert!((target_x - 20.0).abs() < 0.1, "should seek food x");
                assert!((target_y - 10.0).abs() < 0.1, "should seek food y");
            }
            _ => panic!("hungry villager should seek food, got: {:?}", state),
        }
    }

    #[test]
    fn villager_flees_predator() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        let _predator = spawn_predator(&mut world, 12.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(
            matches!(state, BehaviorState::FleeHome { .. }),
            "villager should flee when predator nearby, got: {:?}",
            state
        );
    }

    #[test]
    fn villager_gathers_wood() {
        let mut world = World::new();
        let mut map = walkable_map(30, 30);
        map.set(12, 10, Terrain::Forest);
        map.set(13, 10, Terrain::Forest);

        let villager = spawn_villager(&mut world, 12.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(
            matches!(
                state,
                BehaviorState::Gathering {
                    resource_type: ResourceType::Wood,
                    ..
                }
            ),
            "villager near forest with low hunger should gather wood, got: {:?}",
            state
        );
    }

    #[test]
    fn villager_hauls_to_stockpile() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Gathering {
                timer: 0,
                resource_type: ResourceType::Wood,
            };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        match state {
            BehaviorState::Hauling {
                target_x,
                target_y,
                resource_type,
            } => {
                assert!(
                    (target_x - 5.0).abs() < 0.1,
                    "should haul toward stockpile x"
                );
                assert!(
                    (target_y - 5.0).abs() < 0.1,
                    "should haul toward stockpile y"
                );
                assert_eq!(resource_type, ResourceType::Wood, "should haul wood");
            }
            _ => panic!(
                "villager after gathering should haul to stockpile, got: {:?}",
                state
            ),
        }
    }

    #[test]
    fn villager_deposits_resource() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 5.0, 5.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Hauling {
                target_x: 5.0,
                target_y: 5.0,
                resource_type: ResourceType::Wood,
            };
        }

        let grid = make_grid(&world, &map);
        let result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        assert_eq!(result.deposited.len(), 1, "should deposit one resource");
        assert_eq!(
            result.deposited[0],
            ResourceType::Wood,
            "should deposit wood"
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Idle { .. }),
            "villager should be idle after depositing, got: {:?}",
            state
        );
    }

    #[test]
    fn building_wall_blocks_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::BuildingWall);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "BuildingWall should block movement");
    }

    #[test]
    fn building_floor_is_walkable() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::BuildingFloor);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 5.0, "BuildingFloor should be walkable");
    }

    #[test]
    fn villager_builds_at_site() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);
        let _site = spawn_build_site(&mut world, 10.0, 10.0, BuildingType::Wall, 0);

        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            10, // stockpile_wood (unused for building decision in this test)
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Building { .. }),
            "villager near build site with low hunger should start building, got: {:?}",
            state
        );
    }

    #[test]
    fn build_site_completes() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);
        let site = spawn_build_site(&mut world, 10.0, 10.0, BuildingType::Wall, 0);

        {
            let mut s = world.get::<&mut BuildSite>(site).unwrap();
            s.progress = s.required - 1;
        }

        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Building {
                target_x: 10.0,
                target_y: 10.0,
                timer: 5,
            };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let s = world.get::<&BuildSite>(site).unwrap();
        assert!(
            s.progress >= s.required,
            "build site should be complete: progress={} required={}",
            s.progress,
            s.required
        );
    }

    #[test]
    fn building_type_costs_and_sizes() {
        assert_eq!(
            BuildingType::Hut.cost(),
            Resources {
                wood: 6,
                stone: 3,
                ..Default::default()
            }
        );
        assert_eq!(
            BuildingType::Wall.cost(),
            Resources {
                wood: 2,
                stone: 2,
                ..Default::default()
            }
        );
        assert_eq!(
            BuildingType::Farm.cost(),
            Resources {
                wood: 5,
                stone: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            BuildingType::Stockpile.cost(),
            Resources {
                wood: 4,
                ..Default::default()
            }
        );

        assert_eq!(BuildingType::Hut.size(), (3, 3));
        assert_eq!(BuildingType::Wall.size(), (1, 1));
        assert_eq!(BuildingType::Farm.size(), (3, 3));
        assert_eq!(BuildingType::Stockpile.size(), (2, 2));

        assert_eq!(BuildingType::Hut.build_time(), 180);
        assert_eq!(BuildingType::Wall.build_time(), 45);
        assert_eq!(BuildingType::Farm.build_time(), 120);
        assert_eq!(BuildingType::Stockpile.build_time(), 60);

        let wall_tiles = BuildingType::Wall.tiles();
        assert_eq!(wall_tiles.len(), 1);
        assert_eq!(wall_tiles[0], (0, 0, Terrain::BuildingWall));

        let hut_tiles = BuildingType::Hut.tiles();
        let wall_count = hut_tiles
            .iter()
            .filter(|(_, _, t)| *t == Terrain::BuildingWall)
            .count();
        let floor_count = hut_tiles
            .iter()
            .filter(|(_, _, t)| *t == Terrain::BuildingFloor)
            .count();
        assert!(wall_count > 0, "hut should have wall tiles");
        assert!(floor_count > 0, "hut should have floor tiles");
    }

    #[test]
    fn winter_increases_hunger() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        let start = world.get::<&Creature>(e).unwrap().hunger;

        system_hunger(&mut world, 1.0);
        let normal_hunger = world.get::<&Creature>(e).unwrap().hunger;
        let normal_increase = normal_hunger - start;

        world.get::<&mut Creature>(e).unwrap().hunger = start;
        system_hunger(&mut world, 1.8);
        let winter_hunger = world.get::<&Creature>(e).unwrap().hunger;
        let winter_increase = winter_hunger - start;

        assert!(
            winter_increase > normal_increase,
            "winter hunger increase ({}) should exceed normal ({})",
            winter_increase,
            normal_increase
        );
    }

    #[test]
    fn wolf_attacks_villager_in_winter() {
        let mut world = World::new();
        let map = walkable_map(30, 30);
        let wolf = spawn_predator(&mut world, 10.0, 10.0);
        let _villager = spawn_villager(&mut world, 15.0, 10.0);

        world.get::<&mut Creature>(wolf).unwrap().hunger = 0.9;

        for _ in 0..5 {
            let grid = make_grid(&world, &map);
            system_ai(
                &mut world,
                &map,
                &grid,
                0.8,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            system_movement(&mut world, &map);
        }

        let state = world.get::<&Behavior>(wolf).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Hunting { .. }),
            "hungry wolf should hunt villager in winter, got {:?}",
            state
        );
    }

    #[test]
    fn starvation_kills_creature() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);

        world.get::<&mut Creature>(e).unwrap().hunger = 1.0;

        let dead = system_death(&mut world);
        assert_eq!(dead.len(), 1, "one creature should die");
        assert!(
            world.get::<&Creature>(e).is_err(),
            "dead creature should be despawned"
        );
    }

    #[test]
    fn prey_breeds_in_spring() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
        world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::AtHome { timer: 100 };

        let initial_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Prey)
            .count();
        assert_eq!(initial_count, 1);

        for _ in 0..5000 {
            system_breeding(&mut world, Season::Spring, 1.0, 0);
        }

        let final_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Prey)
            .count();
        assert!(
            final_count > 1,
            "prey should have bred in spring, count={}",
            final_count
        );
    }

    #[test]
    fn predator_breeds_when_fed() {
        let mut world = World::new();
        let e = spawn_predator(&mut world, 15.0, 15.0);
        world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
        world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::Wander { timer: 50 };

        let initial_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        assert_eq!(initial_count, 1);

        for _ in 0..10000 {
            system_breeding(&mut world, Season::Summer, 1.0, 0);
        }

        let final_count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        assert!(
            final_count > 1,
            "wolf should have bred when well-fed, count={}",
            final_count
        );
    }

    #[test]
    fn no_breeding_in_winter() {
        let mut world = World::new();
        let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
        world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
        world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::AtHome { timer: 100 };

        for _ in 0..5000 {
            system_breeding(&mut world, Season::Winter, 1.0, 0);
        }

        let count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Prey)
            .count();
        assert_eq!(count, 1, "no breeding should occur in winter");
    }

    #[test]
    fn prey_population_capped_per_den() {
        let mut world = World::new();
        for _ in 0..3 {
            let e = spawn_prey(&mut world, 10.0, 10.0, 10.0, 10.0);
            world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
            world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::AtHome { timer: 100 };
        }

        for _ in 0..5000 {
            system_breeding(&mut world, Season::Spring, 1.0, 0);
        }

        let count = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Prey)
            .count();
        assert_eq!(
            count, 3,
            "prey should be capped at 3 per den, got {}",
            count
        );
    }

    #[test]
    fn farm_produces_food_with_worker() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);
        let mm = wet_moisture_map();

        for _ in 0..400 {
            for farm in world.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            system_farms(&mut world, Season::Summer, 1.0, &mm);
        }
        let pending = world
            .query::<&FarmPlot>()
            .iter()
            .next()
            .unwrap()
            .pending_food;
        assert!(
            pending >= 3,
            "farm with worker should have produced at least 3 pending food, got {}",
            pending
        );
    }

    #[test]
    fn farm_no_growth_without_worker() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);
        let mm = wet_moisture_map();

        for _ in 0..500 {
            system_farms(&mut world, Season::Summer, 1.0, &mm);
        }

        let growth = world.query::<&FarmPlot>().iter().next().unwrap().growth;
        assert_eq!(
            growth, 0.0,
            "farm should not grow without worker, got {}",
            growth
        );
    }

    #[test]
    fn farm_no_growth_in_winter() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);
        let mm = wet_moisture_map();

        for _ in 0..500 {
            for farm in world.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            system_farms(&mut world, Season::Winter, 1.0, &mm);
        }

        let growth = world.query::<&FarmPlot>().iter().next().unwrap().growth;
        assert_eq!(
            growth, 0.0,
            "farm should not grow in winter even with worker, got {}",
            growth
        );
    }

    #[test]
    fn farm_visual_changes_with_growth() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 10.0);
        let mm = wet_moisture_map();

        let ch = world
            .query::<&Sprite>()
            .iter()
            .find(|s| s.ch == '·')
            .map(|s| s.ch);
        assert_eq!(ch, Some('·'), "new farm should show dirt sprite");

        for _ in 0..150 {
            for farm in world.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            system_farms(&mut world, Season::Summer, 1.0, &mm);
        }
        {
            let mut q = world.query::<(&FarmPlot, &Sprite)>();
            let (_, sprite) = q.iter().next().unwrap();
            assert_eq!(
                sprite.ch, '♠',
                "mid-growth farm should show growing sprite, got '{}'",
                sprite.ch
            );
        }

        for _ in 0..100 {
            for farm in world.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            system_farms(&mut world, Season::Summer, 1.0, &mm);
        }
        {
            let mut q = world.query::<(&FarmPlot, &Sprite)>();
            let (_, sprite) = q.iter().next().unwrap();
            assert_eq!(
                sprite.ch, '"',
                "mature farm should show mature sprite, got '{}'",
                sprite.ch
            );
        }
    }

    #[test]
    fn moisture_ramp_values() {
        // moisture_ramp is private to systems.rs, so we test via system_farms behavior.
        // Farm at moisture=0.0 should grow at 40% of base rate.
        // Farm at moisture=0.6 should grow at 100% of base rate.
        let mut world_dry = World::new();
        spawn_farm_plot(&mut world_dry, 5.0, 5.0);
        let mm_dry = MoistureMap::new(64, 64);
        // moisture defaults to 0.0

        let mut world_wet = World::new();
        spawn_farm_plot(&mut world_wet, 5.0, 5.0);
        let mut mm_wet = MoistureMap::new(64, 64);
        mm_wet.set(5, 5, 0.6);

        let ticks = 100;
        for _ in 0..ticks {
            for farm in world_dry.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            system_farms(&mut world_dry, Season::Summer, 1.0, &mm_dry);
            for farm in world_wet.query_mut::<&mut FarmPlot>() {
                farm.worker_present = true;
            }
            system_farms(&mut world_wet, Season::Summer, 1.0, &mm_wet);
        }

        let dry_growth = world_dry.query::<&FarmPlot>().iter().next().unwrap().growth;
        let wet_growth = world_wet.query::<&FarmPlot>().iter().next().unwrap().growth;
        assert!(
            wet_growth > dry_growth,
            "wet farm should grow faster: wet={}, dry={}",
            wet_growth,
            dry_growth
        );
        // Dry farm grows at 0.003 * 0.4 = 0.0012/tick, 100 ticks = 0.12
        // Wet farm grows at 0.003 * 1.0 = 0.003/tick, 100 ticks = 0.30
        let expected_dry = 0.003 * 0.4 * ticks as f64;
        let expected_wet = 0.003 * 1.0 * ticks as f64;
        assert!(
            (dry_growth - expected_dry).abs() < 0.001,
            "dry farm growth should be ~{}: got {}",
            expected_dry,
            dry_growth
        );
        assert!(
            (wet_growth - expected_wet).abs() < 0.001,
            "wet farm growth should be ~{}: got {}",
            expected_wet,
            wet_growth
        );
    }

    #[test]
    fn farm_tile_position_set_at_spawn() {
        let mut world = World::new();
        spawn_farm_plot(&mut world, 10.0, 20.0);
        let mut q = world.query::<&FarmPlot>();
        let farm = q.iter().next().unwrap();
        assert_eq!(farm.tile_x, 10, "tile_x should match spawn x");
        assert_eq!(farm.tile_y, 20, "tile_y should match spawn y");
    }

    #[test]
    fn villager_settlement_survival() {
        let mut world = World::new();
        let mut map = walkable_map(40, 40);
        for y in 5..10 {
            for x in 5..10 {
                map.set(x, y, Terrain::Forest);
            }
        }

        spawn_stockpile(&mut world, 20.0, 20.0);
        spawn_berry_bush(&mut world, 19.0, 19.0);
        spawn_berry_bush(&mut world, 21.0, 21.0);
        spawn_berry_bush(&mut world, 18.0, 20.0);
        spawn_berry_bush(&mut world, 22.0, 20.0);
        let v1 = spawn_villager(&mut world, 20.0, 21.0);
        let v2 = spawn_villager(&mut world, 21.0, 20.0);
        let v3 = spawn_villager(&mut world, 19.0, 20.0);

        let mut deposits = Vec::new();
        let mut any_gathered = false;

        for tick in 0..3000 {
            system_hunger(&mut world, 1.0);
            let grid = make_grid(&world, &map);
            let r = system_ai(
                &mut world,
                &map,
                &grid,
                0.4,
                0,
                0,
                0,
                0,
                0,
                &SkillMults::default(),
                false,
                false,
                &[],
                0,
            );
            deposits.extend(r.deposited);
            system_movement(&mut world, &map);
            system_death(&mut world);

            for (creature, behavior) in world.query::<(&Creature, &Behavior)>().iter() {
                if creature.species == Species::Villager {
                    if matches!(
                        behavior.state,
                        BehaviorState::Gathering { .. } | BehaviorState::Hauling { .. }
                    ) {
                        any_gathered = true;
                    }
                }
            }
        }

        let final_alive = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count();

        assert!(any_gathered, "villagers should gather from berry bushes");
        assert!(
            final_alive >= 2,
            "at least 2 villagers should survive 3000 ticks, got {}",
            final_alive
        );
    }

    #[test]
    fn villager_eats_from_stockpile_when_no_berries() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let villager = spawn_villager(&mut world, 5.0, 5.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.6;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        let result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            10,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(
            matches!(state, BehaviorState::Eating { .. }),
            "hungry villager near stockpile with food should eat, got: {:?}",
            state
        );
        assert_eq!(
            result.food_consumed, 1,
            "should consume 1 food from stockpile"
        );
    }

    #[test]
    fn villager_gathers_stone_from_deposit() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);
        spawn_stone_deposit(&mut world, 11.0, 10.0);

        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.2;
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Wander { timer: 0 };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let state = world.get::<&Behavior>(villager).unwrap().state;
        assert!(
            matches!(
                state,
                BehaviorState::Gathering {
                    resource_type: ResourceType::Stone,
                    ..
                }
            ),
            "villager near stone deposit with low hunger should gather stone, got: {:?}",
            state
        );
    }

    #[test]
    fn serialize_deserialize_world_round_trip() {
        let mut world = World::new();

        spawn_villager(&mut world, 10.0, 10.0);
        spawn_prey(&mut world, 20.0, 20.0, 15.0, 15.0);
        spawn_predator(&mut world, 30.0, 30.0);
        spawn_berry_bush(&mut world, 5.0, 5.0);
        spawn_stone_deposit(&mut world, 7.0, 7.0);
        spawn_den(&mut world, 15.0, 15.0);
        spawn_stockpile(&mut world, 12.0, 12.0);
        spawn_build_site(&mut world, 8.0, 8.0, BuildingType::Hut, 0);
        spawn_farm_plot(&mut world, 9.0, 9.0);

        let serialized = serialize_world(&world);

        let villager_count = world
            .query::<(&Creature,)>()
            .iter()
            .filter(|(c,)| c.species == Species::Villager)
            .count();
        let prey_count = world
            .query::<(&Creature,)>()
            .iter()
            .filter(|(c,)| c.species == Species::Prey)
            .count();
        let predator_count = world
            .query::<(&Creature,)>()
            .iter()
            .filter(|(c,)| c.species == Species::Predator)
            .count();
        let food_count = world.query::<(&FoodSource,)>().iter().count();
        let stone_count = world.query::<(&StoneDeposit,)>().iter().count();
        let den_count = world.query::<(&Den,)>().iter().count();
        let stockpile_count = world.query::<(&Stockpile,)>().iter().count();
        let build_site_count = world.query::<(&BuildSite,)>().iter().count();
        let farm_count = world.query::<(&FarmPlot,)>().iter().count();

        let new_world = deserialize_world(&serialized);

        assert_eq!(
            new_world
                .query::<(&Creature,)>()
                .iter()
                .filter(|(c,)| c.species == Species::Villager)
                .count(),
            villager_count
        );
        assert_eq!(
            new_world
                .query::<(&Creature,)>()
                .iter()
                .filter(|(c,)| c.species == Species::Prey)
                .count(),
            prey_count
        );
        assert_eq!(
            new_world
                .query::<(&Creature,)>()
                .iter()
                .filter(|(c,)| c.species == Species::Predator)
                .count(),
            predator_count
        );
        assert_eq!(
            new_world.query::<(&FoodSource,)>().iter().count(),
            food_count
        );
        assert_eq!(
            new_world.query::<(&StoneDeposit,)>().iter().count(),
            stone_count
        );
        assert_eq!(new_world.query::<(&Den,)>().iter().count(), den_count);
        assert_eq!(
            new_world.query::<(&Stockpile,)>().iter().count(),
            stockpile_count
        );
        assert_eq!(
            new_world.query::<(&BuildSite,)>().iter().count(),
            build_site_count
        );
        assert_eq!(new_world.query::<(&FarmPlot,)>().iter().count(), farm_count);

        let mut query = new_world.query::<(&Position, &Creature)>();
        let (pos, _creature) = query
            .iter()
            .find(|(_, c)| c.species == Species::Villager)
            .unwrap();
        assert!((pos.x - 10.0).abs() < 0.01);
        assert!((pos.y - 10.0).abs() < 0.01);
    }

    #[test]
    fn workshop_building_type_properties() {
        assert_eq!(
            BuildingType::Workshop.cost(),
            Resources {
                wood: 5,
                stone: 3,
                ..Default::default()
            }
        );
        assert_eq!(BuildingType::Workshop.size(), (3, 3));
        assert_eq!(BuildingType::Workshop.build_time(), 220);
        assert_eq!(BuildingType::Workshop.name(), "Workshop");
    }

    #[test]
    fn smithy_building_type_properties() {
        assert_eq!(
            BuildingType::Smithy.cost(),
            Resources {
                wood: 10,
                stone: 15,
                ..Default::default()
            }
        );
        assert_eq!(BuildingType::Smithy.size(), (3, 3));
        assert_eq!(BuildingType::Smithy.build_time(), 270);
        assert_eq!(BuildingType::Smithy.name(), "Smithy");
    }

    #[test]
    fn system_processing_converts_wood_to_planks() {
        let mut world = World::new();
        spawn_processing_building(&mut world, 5.0, 5.0, Recipe::WoodToPlanks);
        let mut resources = Resources {
            wood: 14,
            ..Default::default()
        };

        for _ in 0..120 {
            for b in world.query_mut::<&mut ProcessingBuilding>() {
                b.worker_present = true;
            }
            system_processing(&mut world, &mut resources, 1.0);
        }

        assert_eq!(resources.wood, 12, "should have consumed 2 wood");
        assert_eq!(resources.planks, 1, "should have produced 1 planks");
    }

    #[test]
    fn system_processing_converts_stone_to_masonry() {
        let mut world = World::new();
        spawn_processing_building(&mut world, 5.0, 5.0, Recipe::StoneToMasonry);
        let mut resources = Resources {
            stone: 4,
            ..Default::default()
        };

        for _ in 0..120 {
            for b in world.query_mut::<&mut ProcessingBuilding>() {
                b.worker_present = true;
            }
            system_processing(&mut world, &mut resources, 1.0);
        }

        assert_eq!(resources.stone, 2, "should have consumed 2 stone");
        assert_eq!(resources.masonry, 1, "should have produced 1 masonry");
    }

    #[test]
    fn system_processing_converts_food_to_grain() {
        let mut world = World::new();
        spawn_processing_building(&mut world, 5.0, 5.0, Recipe::FoodToGrain);
        // Granary only converts when food > 15 (starvation guard). Start with 19 so one
        // conversion (food-=3 → 16 > 15) fires on tick 120, leaving 16 food and 2 grain.
        let mut resources = Resources {
            food: 19,
            ..Default::default()
        };

        for _ in 0..120 {
            for b in world.query_mut::<&mut ProcessingBuilding>() {
                b.worker_present = true;
            }
            system_processing(&mut world, &mut resources, 1.0);
        }

        assert_eq!(
            resources.food, 16,
            "should have consumed 3 food (one conversion at 19→16)"
        );
        assert_eq!(resources.grain, 2, "should have produced 2 grain");
    }

    #[test]
    fn system_processing_no_process_insufficient_resources() {
        let mut world = World::new();
        spawn_processing_building(&mut world, 5.0, 5.0, Recipe::WoodToPlanks);
        let mut resources = Resources {
            wood: 1,
            ..Default::default()
        };

        for _ in 0..120 {
            system_processing(&mut world, &mut resources, 1.0);
        }

        assert_eq!(
            resources.wood, 1,
            "wood should be unchanged with insufficient amount"
        );
        assert_eq!(resources.planks, 0, "no planks should be produced");
    }

    #[test]
    fn new_building_types_in_all_list() {
        let all = BuildingType::all();
        assert!(
            all.contains(&BuildingType::Workshop),
            "all() should contain Workshop"
        );
        assert!(
            all.contains(&BuildingType::Smithy),
            "all() should contain Smithy"
        );
        assert!(
            all.contains(&BuildingType::Granary),
            "all() should contain Granary"
        );
        assert!(
            all.contains(&BuildingType::Bakery),
            "all() should contain Bakery"
        );
    }

    #[test]
    fn bakery_converts_grain_to_bread() {
        let mut world = World::new();
        spawn_processing_building(&mut world, 5.0, 5.0, Recipe::GrainToBread);
        let mut resources = Resources {
            grain: 4,
            planks: 2,
            ..Default::default()
        };

        for _ in 0..150 {
            for b in world.query_mut::<&mut ProcessingBuilding>() {
                b.worker_present = true;
            }
            system_processing(&mut world, &mut resources, 1.0);
        }

        assert!(resources.bread > 0, "bakery should produce bread");
        assert!(resources.grain < 4, "bakery should consume grain");
        assert!(resources.planks < 2, "bakery should consume planks");
    }

    #[test]
    fn granary_and_bakery_building_properties() {
        assert_eq!(BuildingType::Granary.size(), (3, 3));
        assert_eq!(BuildingType::Bakery.size(), (3, 3));
        assert_eq!(BuildingType::Granary.name(), "Granary");
        assert_eq!(BuildingType::Bakery.name(), "Bakery");
        assert!(BuildingType::Granary.cost().wood > 0);
        assert!(BuildingType::Bakery.cost().planks > 0);
    }

    #[test]
    fn workshop_and_smithy_tiles_are_3x3() {
        let workshop_tiles = BuildingType::Workshop.tiles();
        let smithy_tiles = BuildingType::Smithy.tiles();
        assert!(
            workshop_tiles.len() >= 9,
            "workshop should have at least 9 tiles"
        );
        assert!(
            smithy_tiles.len() >= 9,
            "smithy should have at least 9 tiles"
        );
    }

    #[test]
    fn skill_mult_speeds_up_processing() {
        let mut world = World::new();
        spawn_processing_building(&mut world, 5.0, 5.0, Recipe::WoodToPlanks);
        let mut resources = Resources {
            wood: 14,
            ..Default::default()
        };

        for _ in 0..60 {
            for b in world.query_mut::<&mut ProcessingBuilding>() {
                b.worker_present = true;
            }
            system_processing(&mut world, &mut resources, 2.0);
        }

        assert_eq!(
            resources.wood, 12,
            "should have consumed 2 wood at double speed"
        );
        assert_eq!(
            resources.planks, 1,
            "should have produced 1 planks at double speed"
        );
    }

    #[test]
    fn garrison_building_has_correct_cost_and_size() {
        let garrison = BuildingType::Garrison;
        assert_eq!(
            garrison.cost(),
            Resources {
                wood: 6,
                stone: 8,
                ..Default::default()
            },
            "garrison cost should be 6 wood, 8 stone"
        );
        assert_eq!(garrison.size(), (3, 3), "garrison size should be 3x3");
        assert_eq!(
            garrison.build_time(),
            180,
            "garrison build time should be 180"
        );
        assert_eq!(garrison.name(), "Garrison");
        assert!(BuildingType::all().contains(&BuildingType::Garrison));
    }

    #[test]
    fn garrison_tiles_have_wall_perimeter() {
        let tiles = BuildingType::Garrison.tiles();
        let wall_count = tiles
            .iter()
            .filter(|(_, _, t)| *t == Terrain::BuildingWall)
            .count();
        let floor_count = tiles
            .iter()
            .filter(|(_, _, t)| *t == Terrain::BuildingFloor)
            .count();
        assert!(
            wall_count >= 7,
            "garrison should have at least 7 wall tiles, got {}",
            wall_count
        );
        assert!(
            floor_count >= 1,
            "garrison should have at least 1 floor tile, got {}",
            floor_count
        );
    }

    #[test]
    fn spawn_garrison_creates_entity_with_defense_bonus() {
        let mut world = World::new();
        let e = spawn_garrison(&mut world, 10.0, 10.0);
        let garrison = world.get::<&GarrisonBuilding>(e).unwrap();
        assert_eq!(garrison.defense_bonus, 5.0);
        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 10.0);
    }

    #[test]
    fn wolves_repelled_when_settlement_defended() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let wolf = spawn_predator(&mut world, 10.0, 10.0);
        let villager = spawn_villager(&mut world, 12.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut c = world.get::<&mut Creature>(wolf).unwrap();
            c.hunger = 0.7;
        }
        {
            let mut c = world.get::<&mut Creature>(villager).unwrap();
            c.hunger = 0.1;
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            true,
            false,
            &[],
            0,
        );

        let wolf_state = world.get::<&Behavior>(wolf).unwrap().state;
        let is_hunting_villager = match wolf_state {
            BehaviorState::Hunting { target_x, target_y } => {
                let dx = target_x - 12.0;
                let dy = target_y - 10.0;
                dx.abs() < 0.1 && dy.abs() < 0.1
            }
            _ => false,
        };
        assert!(
            !is_hunting_villager,
            "wolf should not hunt villagers when settlement is defended, state: {:?}",
            wolf_state
        );
    }

    #[test]
    fn wolves_can_hunt_when_defense_insufficient() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let wolf = spawn_predator(&mut world, 10.0, 10.0);
        let _villager = spawn_villager(&mut world, 12.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        {
            let mut c = world.get::<&mut Creature>(wolf).unwrap();
            c.hunger = 0.7;
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let wolf_state = world.get::<&Behavior>(wolf).unwrap().state;
        assert!(
            matches!(wolf_state, BehaviorState::Hunting { .. }),
            "wolf should hunt when defense is insufficient, got: {:?}",
            wolf_state
        );
    }

    #[test]
    fn resources_can_afford_and_deduct() {
        let mut res = Resources {
            food: 10,
            wood: 5,
            stone: 3,
            planks: 2,
            masonry: 1,
            grain: 4,
            bread: 0,
        };
        let cost = Resources {
            wood: 3,
            stone: 2,
            ..Default::default()
        };
        assert!(res.can_afford(&cost));
        res.deduct(&cost);
        assert_eq!(res.wood, 2);
        assert_eq!(res.stone, 1);

        let expensive = Resources {
            planks: 10,
            ..Default::default()
        };
        assert!(!res.can_afford(&expensive));
    }

    #[test]
    fn garrison_cost_is_wood_and_stone_only() {
        let cost = BuildingType::Garrison.cost();
        assert_eq!(cost.wood, 6, "garrison should require 6 wood");
        assert_eq!(cost.stone, 8, "garrison should require 8 stone");
        assert_eq!(cost.masonry, 0, "garrison should not require masonry");
        assert_eq!(cost.planks, 0, "garrison should not require planks");

        let sufficient = Resources {
            wood: 6,
            stone: 8,
            ..Default::default()
        };
        assert!(
            sufficient.can_afford(&cost),
            "wood+stone should be sufficient to afford garrison"
        );

        let insufficient = Resources {
            wood: 5,
            stone: 8,
            ..Default::default()
        };
        assert!(
            !insufficient.can_afford(&cost),
            "insufficient wood should not afford garrison"
        );
    }

    #[test]
    fn processing_building_needs_worker() {
        let mut world = World::new();
        let pb = spawn_processing_building(&mut world, 5.0, 5.0, Recipe::WoodToPlanks);

        let mut resources = Resources {
            wood: 14, // >= 12 threshold so has_input=true when worker present
            ..Default::default()
        };
        system_processing(&mut world, &mut resources, 1.0);
        {
            let sprite = world.get::<&Sprite>(pb).unwrap();
            assert_eq!(
                sprite.fg,
                Color(80, 80, 80),
                "should be dark gray without worker"
            );
        }

        {
            let mut building = world.get::<&mut ProcessingBuilding>(pb).unwrap();
            building.worker_present = true;
        }
        system_processing(&mut world, &mut resources, 1.0);
        {
            let sprite = world.get::<&Sprite>(pb).unwrap();
            assert_eq!(
                sprite.fg,
                Color(255, 200, 50),
                "should be bright yellow with worker+inputs"
            );
        }

        resources.wood = 0;
        {
            let mut building = world.get::<&mut ProcessingBuilding>(pb).unwrap();
            building.worker_present = true;
        }
        system_processing(&mut world, &mut resources, 1.0);
        {
            let sprite = world.get::<&Sprite>(pb).unwrap();
            assert_eq!(
                sprite.fg,
                Color(100, 100, 100),
                "should be dim gray when no inputs"
            );
        }
    }

    #[test]
    fn villager_prefers_grain_over_food() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let v = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 10.0, 10.0);

        {
            let mut c = world.get::<&mut Creature>(v).unwrap();
            c.hunger = 0.6;
        }

        let grid = make_grid(&world, &map);
        let result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            5,
            0,
            0,
            5,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        if result.grain_consumed > 0 || result.food_consumed > 0 {
            assert!(
                result.grain_consumed > 0,
                "should prefer grain over raw food"
            );
            assert_eq!(
                result.food_consumed, 0,
                "should not consume food when grain available"
            );
        }
    }

    #[test]
    fn road_building_type_properties() {
        assert_eq!(
            BuildingType::Road.cost(),
            Resources {
                stone: 2,
                ..Default::default()
            }
        );
        assert_eq!(BuildingType::Road.build_time(), 30);
        assert_eq!(BuildingType::Road.size(), (1, 1));
        assert_eq!(BuildingType::Road.tiles(), vec![(0, 0, Terrain::Road)]);
        assert_eq!(BuildingType::Road.name(), "Road");
    }

    #[test]
    fn road_in_all_building_types() {
        assert!(BuildingType::all().contains(&BuildingType::Road));
    }

    #[test]
    fn road_speed_bonus_in_movement() {
        let mut world = World::new();
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        map.set(5, 5, Terrain::Road);
        map.set(6, 5, Terrain::Road);

        let e = world.spawn((Position { x: 5.0, y: 5.0 }, Velocity { dx: 0.1, dy: 0.0 }));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert!(
            (pos.x - 5.15).abs() < 0.001,
            "road should give 1.5x speed: got {}",
            pos.x
        );
    }

    #[test]
    fn grass_no_speed_bonus_in_movement() {
        let mut world = World::new();
        let map = TileMap::new(20, 20, Terrain::Grass);

        let e = world.spawn((Position { x: 5.0, y: 5.0 }, Velocity { dx: 0.1, dy: 0.0 }));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert!(
            (pos.x - 5.1).abs() < 0.001,
            "grass should give 1.0x speed: got {}",
            pos.x
        );
    }

    #[test]
    fn berry_bush_has_resource_yield() {
        let mut world = World::new();
        let bush = spawn_berry_bush(&mut world, 5.0, 5.0);
        let ry = world.get::<&ResourceYield>(bush).unwrap();
        assert_eq!(ry.remaining, 20);
        assert_eq!(ry.max, 20);
    }

    #[test]
    fn stone_deposit_has_resource_yield() {
        let mut world = World::new();
        let stone = spawn_stone_deposit(&mut world, 5.0, 5.0);
        let ry = world.get::<&ResourceYield>(stone).unwrap();
        assert_eq!(ry.remaining, 20);
        assert_eq!(ry.max, 20);
    }

    #[test]
    fn resource_yield_depletes_on_harvest() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let bush = spawn_berry_bush(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 12.0, 10.0);
        let v = spawn_villager(&mut world, 10.0, 10.0);

        {
            let mut b = world.get::<&mut Behavior>(v).unwrap();
            b.state = BehaviorState::Gathering {
                timer: 0,
                resource_type: ResourceType::Food,
            };
        }

        let initial = world.get::<&ResourceYield>(bush).unwrap().remaining;
        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        let after = world.get::<&ResourceYield>(bush).unwrap().remaining;
        assert!(
            after < initial,
            "resource yield should decrease: was {}, now {}",
            initial,
            after
        );
    }

    #[test]
    fn depleted_resource_despawns() {
        let mut world = World::new();
        let map = walkable_map(30, 30);

        let bush = spawn_berry_bush(&mut world, 10.0, 10.0);
        {
            let mut ry = world.get::<&mut ResourceYield>(bush).unwrap();
            ry.remaining = 1;
        }
        spawn_stockpile(&mut world, 12.0, 10.0);
        let v = spawn_villager(&mut world, 10.0, 10.0);
        {
            let mut b = world.get::<&mut Behavior>(v).unwrap();
            b.state = BehaviorState::Gathering {
                timer: 0,
                resource_type: ResourceType::Food,
            };
        }

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        assert!(
            world.get::<&FoodSource>(bush).is_err(),
            "depleted resource should be despawned"
        );
    }

    #[test]
    fn stone_does_not_regrow() {
        let mut world = World::new();
        let mut map = walkable_map(30, 30);

        for tick in 0..10 {
            let veg = crate::simulation::VegetationMap::new(30, 30);
            system_regrowth(&mut world, &mut map, &veg, tick * 400);
        }

        let stone_count = world.query::<&StoneDeposit>().iter().count();
        assert_eq!(stone_count, 0, "stone should not regrow");
    }

    #[test]
    fn wood_harvest_converts_forest_to_stump() {
        // When a villager finishes gathering wood (timer hits 0), system_ai should
        // report the harvest position so the caller can convert Forest -> Stump.
        let mut world = World::new();
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        map.set(10, 10, Terrain::Forest);
        map.set(11, 10, Terrain::Forest);

        let villager = spawn_villager(&mut world, 10.0, 10.0);
        spawn_stockpile(&mut world, 5.0, 5.0);

        // Put villager in Gathering Wood state with timer at 0 (ready to transition to Hauling)
        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Gathering {
                timer: 0,
                resource_type: ResourceType::Wood,
            };
        }

        let grid = make_grid(&world, &map);
        let result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        assert!(
            !result.wood_harvest_positions.is_empty(),
            "should report wood harvest position"
        );

        // Simulate what Game::step does: convert Forest -> Stump at harvest position
        for (hx, hy) in &result.wood_harvest_positions {
            let ix = hx.round() as usize;
            let iy = hy.round() as usize;
            if map.get(ix, iy) == Some(&Terrain::Forest) {
                map.set(ix, iy, Terrain::Stump);
            }
        }

        assert_eq!(
            map.get(10, 10).copied(),
            Some(Terrain::Stump),
            "harvested forest tile should become stump"
        );
    }

    #[test]
    fn stump_decays_to_bare() {
        let mut world = World::new();
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        // Place many stumps so random sampling hits at least one
        for x in 0..30 {
            for y in 0..30 {
                map.set(x, y, Terrain::Stump);
            }
        }
        let veg = crate::simulation::VegetationMap::new(30, 30);

        // Run regrowth many times — 30% chance per check, with 900 stumps and 20 samples
        // per check, statistically guaranteed to convert at least one.
        for tick in 0..50 {
            system_regrowth(&mut world, &mut map, &veg, tick * 400);
        }

        let mut found_bare = false;
        for y in 0..30 {
            for x in 0..30 {
                if map.get(x, y) == Some(&Terrain::Bare) {
                    found_bare = true;
                }
            }
        }
        assert!(found_bare, "some stumps should have decayed to bare ground");
    }

    #[test]
    fn bare_adjacent_to_forest_becomes_sapling() {
        let mut world = World::new();
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        // Create a bare area adjacent to forest
        for x in 5..15 {
            for y in 5..15 {
                map.set(x, y, Terrain::Bare);
            }
        }
        // Forest border along the edge
        for x in 4..16 {
            map.set(x, 4, Terrain::Forest);
        }

        // VegetationMap with high moisture so regrowth is not gated
        let mut veg = crate::simulation::VegetationMap::new(30, 30);
        for y in 0..30 {
            for x in 0..30 {
                if let Some(v) = veg.get_mut(x, y) {
                    *v = 0.5;
                }
            }
        }

        // Run many ticks — 5% chance per adjacent-to-forest bare tile
        for tick in 0..200 {
            system_regrowth(&mut world, &mut map, &veg, tick * 400);
        }

        let mut found_sapling = false;
        for y in 5..15 {
            for x in 5..15 {
                if map.get(x, y) == Some(&Terrain::Sapling) {
                    found_sapling = true;
                }
            }
        }
        assert!(
            found_sapling,
            "bare tiles adjacent to forest should eventually sprout saplings"
        );
    }

    #[test]
    fn isolated_bare_does_not_regrow() {
        let mut world = World::new();
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Single bare tile with no forest or sapling neighbors
        map.set(5, 5, Terrain::Bare);

        let mut veg = crate::simulation::VegetationMap::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                if let Some(v) = veg.get_mut(x, y) {
                    *v = 0.5;
                }
            }
        }

        for tick in 0..100 {
            system_regrowth(&mut world, &mut map, &veg, tick * 400);
        }

        // With only 1 bare tile in a 10x10 map, random sampling might miss it.
        // But even if hit, it has no adjacent forest/sapling, so chance is 0%.
        // The tile should still be Bare or Grass (never Sapling).
        let terrain = map.get(5, 5).copied();
        assert!(
            terrain != Some(Terrain::Sapling) && terrain != Some(Terrain::Forest),
            "isolated bare tile should not become sapling or forest, got: {:?}",
            terrain
        );
    }

    #[test]
    fn sapling_converts_to_forest() {
        let mut world = World::new();
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        // Fill with saplings so random sampling hits them
        for x in 0..30 {
            for y in 0..30 {
                map.set(x, y, Terrain::Sapling);
            }
        }
        let veg = crate::simulation::VegetationMap::new(30, 30);

        // 3% chance per check, 20 samples per check — run many ticks
        for tick in 0..100 {
            system_regrowth(&mut world, &mut map, &veg, tick * 400);
        }

        let mut found_forest = false;
        for y in 0..30 {
            for x in 0..30 {
                if map.get(x, y) == Some(&Terrain::Forest) {
                    found_forest = true;
                }
            }
        }
        assert!(
            found_forest,
            "some saplings should have matured into forest"
        );
    }

    #[test]
    fn bare_low_moisture_does_not_sprout() {
        let mut world = World::new();
        let mut map = TileMap::new(30, 30, Terrain::Grass);
        // Bare area adjacent to forest
        for x in 5..15 {
            for y in 5..15 {
                map.set(x, y, Terrain::Bare);
            }
        }
        for x in 4..16 {
            map.set(x, 4, Terrain::Forest);
        }

        // VegetationMap with zero moisture — regrowth gated on > 0.2
        let veg = crate::simulation::VegetationMap::new(30, 30);

        for tick in 0..200 {
            system_regrowth(&mut world, &mut map, &veg, tick * 400);
        }

        let mut found_sapling = false;
        for y in 5..15 {
            for x in 5..15 {
                if map.get(x, y) == Some(&Terrain::Sapling) {
                    found_sapling = true;
                }
            }
        }
        assert!(
            !found_sapling,
            "bare tiles in low-moisture areas should not sprout saplings"
        );
    }

    #[test]
    fn new_terrain_variants_properties() {
        // Verify basic properties of Stump, Bare, Sapling
        assert!(Terrain::Stump.is_walkable());
        assert!(Terrain::Bare.is_walkable());
        assert!(Terrain::Sapling.is_walkable());

        assert_eq!(Terrain::Stump.ch(), '%');
        assert_eq!(Terrain::Bare.ch(), '.');
        assert_eq!(Terrain::Sapling.ch(), '!');

        // Speed: Bare > Stump > Sapling > Forest
        assert!(Terrain::Bare.speed_multiplier() > Terrain::Stump.speed_multiplier());
        assert!(Terrain::Stump.speed_multiplier() > Terrain::Sapling.speed_multiplier());
        assert!(Terrain::Sapling.speed_multiplier() > Terrain::Forest.speed_multiplier());

        // Cost: Forest > Sapling > Stump > Bare
        assert!(Terrain::Forest.move_cost() > Terrain::Sapling.move_cost());
        assert!(Terrain::Sapling.move_cost() > Terrain::Stump.move_cost());
        assert!(Terrain::Stump.move_cost() > Terrain::Bare.move_cost());

        // All have bg colors
        assert!(Terrain::Stump.bg().is_some());
        assert!(Terrain::Bare.bg().is_some());
        assert!(Terrain::Sapling.bg().is_some());
    }

    #[test]
    fn resource_yield_serialization_round_trip() {
        let mut world = World::new();
        spawn_berry_bush(&mut world, 5.0, 5.0);
        spawn_stone_deposit(&mut world, 10.0, 10.0);

        for (_, mut ry) in world.query::<(&FoodSource, &mut ResourceYield)>().iter() {
            ry.remaining = 15;
        }

        let serialized = serialize_world(&world);
        let world2 = deserialize_world(&serialized);

        let bush_yield: Vec<u32> = world2
            .query::<(&FoodSource, &ResourceYield)>()
            .iter()
            .map(|(_, ry)| ry.remaining)
            .collect();
        assert_eq!(bush_yield, vec![15], "bush yield should round-trip");

        let stone_yield: Vec<u32> = world2
            .query::<(&StoneDeposit, &ResourceYield)>()
            .iter()
            .map(|(_, ry)| ry.remaining)
            .collect();
        assert_eq!(stone_yield, vec![20], "stone yield should round-trip");
    }

    #[test]
    fn wolf_cap_scales_with_year() {
        // Year 0: cap = 4 + 2*0 = 4
        let mut world = World::new();
        for _ in 0..5 {
            let e = spawn_predator(&mut world, 15.0, 15.0);
            world.get::<&mut Creature>(e).unwrap().hunger = 0.1;
            world.get::<&mut Behavior>(e).unwrap().state = BehaviorState::Wander { timer: 50 };
        }
        // At year 0, cap is 4 — breeding should not increase past 4
        // We already have 5 wolves, so no breeding should occur
        let before = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        for _ in 0..5000 {
            system_breeding(&mut world, Season::Summer, 1.0, 0);
        }
        let after = world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        assert_eq!(
            after, before,
            "year 0: wolves above cap 4 should not breed, got {}",
            after
        );

        // Year 2: cap = 4 + 2*2 = 8
        let mut world2 = World::new();
        for _ in 0..5 {
            let e = spawn_predator(&mut world2, 15.0, 15.0);
            world2.get::<&mut Creature>(e).unwrap().hunger = 0.1;
            world2.get::<&mut Behavior>(e).unwrap().state = BehaviorState::Wander { timer: 50 };
        }
        // At year 2, cap is 8 — 5 wolves should be able to breed
        for _ in 0..10000 {
            system_breeding(&mut world2, Season::Summer, 1.0, 2);
        }
        let after2 = world2
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Predator)
            .count();
        assert!(
            after2 > 5,
            "year 2: wolves below cap 8 should breed, got {}",
            after2
        );
    }

    #[test]
    fn raid_threshold_decreases_over_time() {
        // Year 0: threshold = max(3, 5-0) = 5, so 4 wolves should NOT raid
        let mut world = World::new();
        for i in 0..4 {
            spawn_predator(&mut world, 30.0 + i as f64, 30.0);
        }
        let raided = system_wolf_raids(&mut world, 25.0, 25.0, 50, 0);
        assert!(!raided, "year 0: 4 wolves should not raid (threshold 5)");

        // Year 2: threshold = max(3, 5-2) = 3, so 4 wolves SHOULD raid
        let mut world2 = World::new();
        for i in 0..4 {
            spawn_predator(&mut world2, 30.0 + i as f64, 30.0);
        }
        let raided2 = system_wolf_raids(&mut world2, 25.0, 25.0, 50, 2);
        assert!(raided2, "year 2: 4 wolves should raid (threshold 3)");

        // Year 10: threshold = max(3, 5-10) = 3 (clamped), so 3 wolves SHOULD raid
        let mut world3 = World::new();
        for i in 0..3 {
            spawn_predator(&mut world3, 30.0 + i as f64, 30.0);
        }
        let raided3 = system_wolf_raids(&mut world3, 25.0, 25.0, 50, 10);
        assert!(
            raided3,
            "year 10: 3 wolves should raid (threshold 3, clamped)"
        );
    }

    #[test]
    fn mining_terrain_quarry_quarrydeep_properties() {
        // Verify basic properties of Quarry, QuarryDeep, ScarredGround
        assert!(Terrain::Quarry.is_walkable());
        assert!(Terrain::QuarryDeep.is_walkable());
        assert!(Terrain::ScarredGround.is_walkable());

        assert_eq!(Terrain::Quarry.ch(), 'U');
        assert_eq!(Terrain::QuarryDeep.ch(), 'V');
        assert_eq!(Terrain::ScarredGround.ch(), '.');

        // Speed: ScarredGround > Quarry > QuarryDeep
        assert!(Terrain::ScarredGround.speed_multiplier() > Terrain::Quarry.speed_multiplier());
        assert!(Terrain::Quarry.speed_multiplier() > Terrain::QuarryDeep.speed_multiplier());

        // Cost: QuarryDeep > Quarry > ScarredGround
        assert!(Terrain::QuarryDeep.move_cost() > Terrain::Quarry.move_cost());
        assert!(Terrain::Quarry.move_cost() > Terrain::ScarredGround.move_cost());

        // All have bg colors
        assert!(Terrain::Quarry.bg().is_some());
        assert!(Terrain::QuarryDeep.bg().is_some());
        assert!(Terrain::ScarredGround.bg().is_some());
    }

    #[test]
    fn stone_deposit_depletion_reports_position() {
        // When a StoneDeposit is depleted, system_ai should report its position
        // so the caller can set ScarredGround.
        let mut world = World::new();
        let mut map = TileMap::new(30, 30, Terrain::Grass);

        // Create a stone deposit with 1 remaining
        spawn_stone_deposit(&mut world, 10.0, 10.0);
        // Set remaining to 1 so it depletes on next harvest
        for (_, mut ry) in world.query::<(&StoneDeposit, &mut ResourceYield)>().iter() {
            ry.remaining = 1;
        }

        let villager = spawn_villager(&mut world, 10.0, 11.0);
        spawn_stockpile(&mut world, 15.0, 15.0);

        // Put villager in Gathering Stone state with timer at 0 (ready to transition to Hauling)
        {
            let mut b = world.get::<&mut Behavior>(villager).unwrap();
            b.state = BehaviorState::Gathering {
                timer: 0,
                resource_type: ResourceType::Stone,
            };
        }

        let grid = make_grid(&world, &map);
        let result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            0,
        );

        assert!(
            !result.depleted_stone_positions.is_empty(),
            "should report depleted stone deposit position"
        );

        // Simulate what Game::step does: convert to ScarredGround
        for (sx, sy) in &result.depleted_stone_positions {
            let ix = sx.round() as usize;
            let iy = sy.round() as usize;
            map.set(ix, iy, Terrain::ScarredGround);
        }

        assert_eq!(
            map.get(10, 10).copied(),
            Some(Terrain::ScarredGround),
            "depleted stone deposit tile should become ScarredGround"
        );
    }

    // --- Phase 0: StockpileFullness ---

    #[test]
    fn stockpile_fullness_from_count() {
        assert_eq!(StockpileFullness::from_count(0), StockpileFullness::Empty);
        assert_eq!(StockpileFullness::from_count(1), StockpileFullness::Low);
        assert_eq!(StockpileFullness::from_count(4), StockpileFullness::Low);
        assert_eq!(StockpileFullness::from_count(5), StockpileFullness::Medium);
        assert_eq!(StockpileFullness::from_count(20), StockpileFullness::Medium);
        assert_eq!(StockpileFullness::from_count(21), StockpileFullness::High);
        assert_eq!(StockpileFullness::from_count(100), StockpileFullness::High);
    }

    #[test]
    fn stockpile_fullness_is_scarce() {
        assert!(StockpileFullness::Empty.is_scarce());
        assert!(StockpileFullness::Low.is_scarce());
        assert!(!StockpileFullness::Medium.is_scarce());
        assert!(!StockpileFullness::High.is_scarce());
    }

    #[test]
    fn stockpile_state_computed_before_ai() {
        // Verify that StockpileState is correctly constructed from resource counts
        let state = StockpileState {
            food: StockpileFullness::from_count(0),
            wood: StockpileFullness::from_count(3),
            stone: StockpileFullness::from_count(25),
        };
        assert_eq!(state.food, StockpileFullness::Empty);
        assert_eq!(state.wood, StockpileFullness::Low);
        assert_eq!(state.stone, StockpileFullness::High);
    }

    // --- Phase 1: Sight-range filtering ---

    #[test]
    fn villager_does_not_seek_hut_beyond_sight_range() {
        // Place a villager and a hut far apart; villager should sleep outdoors
        let mut world = World::new();
        let map = walkable_map(200, 200);

        // Villager at (10, 10) with sight_range 22
        let v = spawn_villager(&mut world, 10.0, 10.0);

        // Hut at (100, 100) — well beyond sight range
        let _hut = world.spawn((
            Position { x: 100.0, y: 100.0 },
            HutBuilding {
                capacity: 4,
                occupants: 0,
            },
        ));

        let grid = make_grid(&world, &map);
        // Run AI at night — villager should sleep outdoors (timer 100) not seek hut
        let _result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            10,
            10,
            10,
            0,
            0,
            &SkillMults::default(),
            false,
            true, // is_night
            &[],
            0,
        );

        let behavior = world.get::<&Behavior>(v).unwrap();
        // Should be sleeping outdoors (timer=100) not seeking a hut
        match behavior.state {
            BehaviorState::Sleeping { timer } => {
                assert_eq!(timer, 100, "should sleep outdoors with short timer");
            }
            _ => panic!("expected Sleeping outdoors, got {:?}", behavior.state),
        }
    }

    #[test]
    fn villager_seeks_hut_within_sight_range() {
        let mut world = World::new();
        let map = walkable_map(200, 200);

        // Villager at (10, 10) with sight_range 22
        let v = spawn_villager(&mut world, 10.0, 10.0);

        // Hut at (20, 10) — within sight range (distance 10)
        let _hut = world.spawn((
            Position { x: 20.0, y: 10.0 },
            HutBuilding {
                capacity: 4,
                occupants: 0,
            },
        ));

        let grid = make_grid(&world, &map);
        let _result = system_ai(
            &mut world,
            &map,
            &grid,
            0.4,
            10,
            10,
            10,
            0,
            0,
            &SkillMults::default(),
            false,
            true, // is_night
            &[],
            0,
        );

        let behavior = world.get::<&Behavior>(v).unwrap();
        // Should be seeking the hut or sleeping in it
        match behavior.state {
            BehaviorState::Seek {
                reason: SeekReason::Hut,
                ..
            } => {} // correct: heading to the hut
            BehaviorState::Sleeping { timer: 200 } => {} // correct: arrived at hut
            other => panic!("expected Seek{{Hut}} or Sleeping{{200}}, got {:?}", other),
        }
    }

    // --- Path caching tests ---

    #[test]
    fn path_cache_populated_on_villager_movement() {
        let mut world = World::new();
        let map = walkable_map(40, 40);
        let v = spawn_villager(&mut world, 5.0, 5.0);
        spawn_stockpile(&mut world, 30.0, 30.0);

        // Make villager hungry so it seeks food/stockpile
        world.get::<&mut Creature>(v).unwrap().hunger = 0.6;

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            10,
            10,
            10,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            100,
        );

        // Villager should have a PathCache with content
        let cache = world.get::<&PathCache>(v).unwrap();
        // If villager is seeking something far away, cache should have waypoints
        let behavior = world.get::<&Behavior>(v).unwrap();
        if matches!(behavior.state, BehaviorState::Seek { .. }) {
            assert!(
                !cache.waypoints.is_empty() || cache.computed_tick == 0,
                "cache should be populated or be default for short distances"
            );
        }
    }

    #[test]
    fn path_cache_reused_across_ticks() {
        let mut world = World::new();
        let map = walkable_map(50, 50);
        let v = spawn_villager(&mut world, 5.0, 5.0);
        spawn_stockpile(&mut world, 40.0, 40.0);

        // Force hauling state with a distant target to ensure cache usage
        world.get::<&mut Behavior>(v).unwrap().state = BehaviorState::Hauling {
            target_x: 40.0,
            target_y: 40.0,
            resource_type: ResourceType::Wood,
        };

        // Tick 100: first call computes cache
        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            100,
        );
        system_movement(&mut world, &map);

        let cache_after_first = (*world.get::<&PathCache>(v).unwrap()).clone();
        assert!(
            !cache_after_first.waypoints.is_empty(),
            "cache should have waypoints after first tick"
        );
        assert_eq!(cache_after_first.computed_tick, 100);

        // Tick 101: cache should be reused (same destination, not stale)
        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            101,
        );

        let cache_after_second = (*world.get::<&PathCache>(v).unwrap()).clone();
        // computed_tick should still be 100 (reused, not recomputed)
        assert_eq!(
            cache_after_second.computed_tick, 100,
            "cache should be reused, not recomputed"
        );
    }

    #[test]
    fn path_cache_invalidated_on_destination_change() {
        let mut world = World::new();
        let map = walkable_map(50, 50);
        let v = spawn_villager(&mut world, 5.0, 5.0);
        spawn_stockpile(&mut world, 40.0, 40.0);

        // First destination
        world.get::<&mut Behavior>(v).unwrap().state = BehaviorState::Hauling {
            target_x: 40.0,
            target_y: 40.0,
            resource_type: ResourceType::Wood,
        };

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            100,
        );

        let cache_first = (*world.get::<&PathCache>(v).unwrap()).clone();
        assert_eq!(cache_first.dest_x, 40.0);

        // Change destination
        world.get::<&mut Behavior>(v).unwrap().state = BehaviorState::Hauling {
            target_x: 10.0,
            target_y: 10.0,
            resource_type: ResourceType::Stone,
        };

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            101,
        );

        let cache_second = (*world.get::<&PathCache>(v).unwrap()).clone();
        // Should have been recomputed for new destination
        assert_eq!(cache_second.computed_tick, 101);
        assert!((cache_second.dest_x - 10.0).abs() < 0.5);
    }

    #[test]
    fn path_cache_invalidated_on_staleness() {
        let mut world = World::new();
        let map = walkable_map(50, 50);
        let v = spawn_villager(&mut world, 5.0, 5.0);

        // Set up hauling with a path cache that was computed long ago
        world.get::<&mut Behavior>(v).unwrap().state = BehaviorState::Hauling {
            target_x: 40.0,
            target_y: 40.0,
            resource_type: ResourceType::Wood,
        };

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            100,
        );

        let cache_first = (*world.get::<&PathCache>(v).unwrap()).clone();
        assert_eq!(cache_first.computed_tick, 100);

        // Jump to tick 300 (>120 ticks stale)
        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            300,
        );

        let cache_after = (*world.get::<&PathCache>(v).unwrap()).clone();
        // Should have been recomputed due to staleness
        assert_eq!(
            cache_after.computed_tick, 300,
            "cache should be recomputed after staleness timeout"
        );
    }

    #[test]
    fn path_cache_short_distance_bypasses_cache() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let v = spawn_villager(&mut world, 5.0, 5.0);

        // Hauling to very close target (d < 3.0)
        world.get::<&mut Behavior>(v).unwrap().state = BehaviorState::Hauling {
            target_x: 6.0,
            target_y: 5.0,
            resource_type: ResourceType::Wood,
        };

        let grid = make_grid(&world, &map);
        system_ai(
            &mut world,
            &map,
            &grid,
            0.0,
            0,
            0,
            0,
            0,
            0,
            &SkillMults::default(),
            false,
            false,
            &[],
            100,
        );

        let cache = world.get::<&PathCache>(v).unwrap();
        // Cache should remain empty/default for short distances
        assert!(
            cache.waypoints.is_empty(),
            "short distance should bypass cache"
        );
    }
}
