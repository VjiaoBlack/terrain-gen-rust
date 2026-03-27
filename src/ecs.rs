use hecs::World;
use rand::RngExt;
use serde::Serialize;

use crate::renderer::{Color, Renderer};
use crate::tilemap::TileMap;

// --- Components ---

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Velocity {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Sprite {
    pub ch: char,
    pub fg: Color,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum BehaviorState {
    /// Wander randomly. Timer counts down to next direction change.
    Wander { timer: u32 },
    /// Move toward a target position.
    Seek { target_x: f64, target_y: f64 },
    /// Stand still. Timer counts down before switching to Wander.
    Idle { timer: u32 },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Behavior {
    pub state: BehaviorState,
    pub speed: f64,
}

// --- Systems ---

/// Move entities with terrain collision. Each axis is tested independently so
/// entities slide along walls. If blocked, velocity on that axis is reversed
/// (NPCs bounce).
pub fn system_movement(world: &mut World, map: &TileMap) {
    for (pos, vel) in world.query_mut::<(&mut Position, &mut Velocity)>() {
        // Try X
        let new_x = pos.x + vel.dx;
        if map.is_walkable(new_x, pos.y) {
            pos.x = new_x;
        } else {
            vel.dx = -vel.dx; // bounce
        }
        // Try Y
        let new_y = pos.y + vel.dy;
        if map.is_walkable(pos.x, new_y) {
            pos.y = new_y;
        } else {
            vel.dy = -vel.dy; // bounce
        }
    }
}


/// AI system: updates velocity based on behavior state and terrain.
pub fn system_ai(world: &mut World, map: &TileMap) {
    let mut rng = rand::rng();

    // 8 cardinal + diagonal directions
    const DIRS: [(f64, f64); 8] = [
        (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
        (1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0),
    ];

    for (pos, vel, behavior) in world.query_mut::<(&Position, &mut Velocity, &mut Behavior)>() {
        match &mut behavior.state {
            BehaviorState::Wander { timer } => {
                if *timer == 0 {
                    // Pick a random walkable direction
                    let mut candidates: Vec<(f64, f64)> = Vec::new();
                    for &(dx, dy) in &DIRS {
                        if map.is_walkable(pos.x + dx * 2.0, pos.y + dy * 2.0) {
                            candidates.push((dx, dy));
                        }
                    }
                    if let Some(&(dx, dy)) = candidates.get(rng.random_range(0..candidates.len().max(1))) {
                        let len: f64 = (dx * dx + dy * dy).sqrt();
                        vel.dx = dx / len * behavior.speed;
                        vel.dy = dy / len * behavior.speed;
                    } else {
                        vel.dx = 0.0;
                        vel.dy = 0.0;
                    }
                    // Wander for 20-60 ticks, then maybe idle
                    *timer = rng.random_range(20..60);

                    // 20% chance to idle instead
                    if rng.random_range(0..5) == 0 {
                        vel.dx = 0.0;
                        vel.dy = 0.0;
                        behavior.state = BehaviorState::Idle { timer: rng.random_range(30..90) };
                    }
                } else {
                    *timer -= 1;
                }
            }
            BehaviorState::Seek { target_x, target_y } => {
                let dx = *target_x - pos.x;
                let dy = *target_y - pos.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < 1.5 {
                    // Arrived — switch to idle
                    vel.dx = 0.0;
                    vel.dy = 0.0;
                    behavior.state = BehaviorState::Idle { timer: rng.random_range(30..90) };
                } else {
                    // Greedy: move toward target, prefer walkable
                    vel.dx = (dx / dist) * behavior.speed;
                    vel.dy = (dy / dist) * behavior.speed;
                }
            }
            BehaviorState::Idle { timer } => {
                vel.dx = 0.0;
                vel.dy = 0.0;
                if *timer == 0 {
                    behavior.state = BehaviorState::Wander { timer: 0 };
                } else {
                    *timer -= 1;
                }
            }
        }
    }
}

pub fn system_render(world: &World, renderer: &mut dyn Renderer) {
    for (pos, sprite) in world.query::<(&Position, &Sprite)>().iter() {
        let x = pos.x.round() as i32;
        let y = pos.y.round() as i32;
        if x >= 0 && y >= 0 {
            renderer.draw(x as u16, y as u16, sprite.ch, sprite.fg, None);
        }
    }
}

// --- Helpers ---

pub fn spawn_entity(world: &mut World, x: f64, y: f64, dx: f64, dy: f64, ch: char, fg: Color) -> hecs::Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx, dy },
        Sprite { ch, fg },
    ))
}

pub fn spawn_npc(world: &mut World, x: f64, y: f64, speed: f64, ch: char, fg: Color) -> hecs::Entity {
    world.spawn((
        Position { x, y },
        Velocity { dx: 0.0, dy: 0.0 },
        Sprite { ch, fg },
        Behavior {
            state: BehaviorState::Wander { timer: 0 },
            speed,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_renderer::HeadlessRenderer;
    use crate::tilemap::{Terrain, TileMap};

    fn walkable_map(w: usize, h: usize) -> TileMap {
        TileMap::new(w, h, Terrain::Grass)
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
        // Wall of mountains at x=5
        for y in 0..10 {
            map.set(5, y, Terrain::Mountain);
        }
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        // Should be blocked, position unchanged on x
        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "should be blocked by mountain wall");
    }

    #[test]
    fn collision_bounces_velocity() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::Mountain);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let vel = world.get::<&Velocity>(e).unwrap();
        assert_eq!(vel.dx, -1.0, "velocity should bounce on collision");
    }

    #[test]
    fn slides_along_wall() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        // Wall at x=5
        for y in 0..10 {
            map.set(5, y, Terrain::Mountain);
        }
        // Moving diagonally into wall
        let e = spawn_entity(&mut world, 4.0, 4.0, 1.0, 1.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "x should be blocked");
        assert_eq!(pos.y, 5.0, "y should still move (slide)");
    }

    #[test]
    fn water_blocks_movement() {
        let mut world = World::new();
        let mut map = walkable_map(10, 10);
        map.set(5, 5, Terrain::Water);
        let e = spawn_entity(&mut world, 4.0, 5.0, 1.0, 0.0, '@', Color(255, 255, 255));

        system_movement(&mut world, &map);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 4.0, "water should block movement");
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

        // nothing should have been drawn
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
        let map = walkable_map(20, 20);
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.2, '☺', Color(200, 100, 50));

        let start_pos = *world.get::<&Position>(e).unwrap();

        // Run AI + movement for enough ticks that wander should pick a direction
        for _ in 0..100 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
        }

        let end_pos = *world.get::<&Position>(e).unwrap();
        let dist = ((end_pos.x - start_pos.x).powi(2) + (end_pos.y - start_pos.y).powi(2)).sqrt();
        assert!(dist > 1.0, "NPC should have moved from spawn: dist={}", dist);
    }

    #[test]
    fn npc_stays_on_walkable_terrain() {
        let mut world = World::new();
        // Island: grass in center, water everywhere else
        let mut map = TileMap::new(20, 20, Terrain::Water);
        for y in 5..15 {
            for x in 5..15 {
                map.set(x, y, Terrain::Grass);
            }
        }
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.3, '☺', Color(200, 100, 50));

        for _ in 0..500 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
        }

        let pos = *world.get::<&Position>(e).unwrap();
        assert!(map.is_walkable(pos.x, pos.y),
            "NPC should stay on walkable terrain: pos=({}, {})", pos.x, pos.y);
    }

    #[test]
    fn idle_npc_stays_still() {
        let mut world = World::new();
        let map = walkable_map(20, 20);
        let e = spawn_npc(&mut world, 10.0, 10.0, 0.2, '☺', Color(200, 100, 50));

        // Force into idle state
        {
            let mut behavior = world.get::<&mut Behavior>(e).unwrap();
            behavior.state = BehaviorState::Idle { timer: 100 };
        }

        let start_pos = *world.get::<&Position>(e).unwrap();

        for _ in 0..50 {
            system_ai(&mut world, &map);
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

        // Force into seek state
        {
            let mut behavior = world.get::<&mut Behavior>(e).unwrap();
            behavior.state = BehaviorState::Seek { target_x: 15.0, target_y: 15.0 };
        }

        for _ in 0..100 {
            system_ai(&mut world, &map);
            system_movement(&mut world, &map);
        }

        let pos = *world.get::<&Position>(e).unwrap();
        let dist = ((pos.x - 15.0).powi(2) + (pos.y - 15.0).powi(2)).sqrt();
        assert!(dist < 2.0, "NPC should reach target: dist={}, pos=({}, {})", dist, pos.x, pos.y);
    }
}
