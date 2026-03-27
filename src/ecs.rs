use hecs::World;
use serde::Serialize;

use crate::renderer::{Color, Renderer};

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

// --- Systems ---

pub fn system_movement(world: &mut World) {
    for (pos, vel) in world.query_mut::<(&mut Position, &Velocity)>() {
        pos.x += vel.dx;
        pos.y += vel.dy;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_renderer::HeadlessRenderer;

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
        let e = spawn_entity(&mut world, 10.0, 5.0, 1.5, -0.5, '@', Color(255, 255, 255));

        system_movement(&mut world);

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 11.5);
        assert_eq!(pos.y, 4.5);
    }

    #[test]
    fn movement_accumulates_over_ticks() {
        let mut world = World::new();
        let e = spawn_entity(&mut world, 0.0, 0.0, 1.0, 1.0, '@', Color(255, 255, 255));

        for _ in 0..10 {
            system_movement(&mut world);
        }

        let pos = world.get::<&Position>(e).unwrap();
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 10.0);
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
}
