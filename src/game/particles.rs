use rand::RngExt;

use crate::ecs::{Behavior, BehaviorState, Position, ProcessingBuilding, Recipe, ResourceType};
use crate::renderer::Color;

use super::{MAX_PARTICLES, OverlayMode, Particle};

impl super::Game {
    /// Update existing particles: move, age, and remove dead ones.
    /// Mark old + new positions dirty for rendering.
    pub(super) fn update_particles(&mut self) {
        for p in &mut self.particles {
            let old_x = p.x.round() as usize;
            let old_y = p.y.round() as usize;
            self.dirty.mark(old_x, old_y);
            p.x += p.dx;
            p.y += p.dy;
            p.life -= 1;
            let new_x = p.x.round() as usize;
            let new_y = p.y.round() as usize;
            self.dirty.mark(new_x, new_y);
        }
        // Mark expired particles' positions dirty before removing
        for p in &self.particles {
            if p.life == 0 {
                self.dirty.mark(p.x.round() as usize, p.y.round() as usize);
            }
        }
        self.particles.retain(|p| p.life > 0);
    }

    /// Spawn wind flow particles when the WindFlow overlay is active.
    pub(super) fn spawn_wind_particles(&mut self) {
        if self.overlay != OverlayMode::WindFlow {
            return;
        }

        let mut rng = rand::rng();
        let vw = 80i32;
        let vh = 50i32;
        // Spawn many particles every tick for visible flow
        for _ in 0..15 {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let px = self.camera.x + (rng.random_range(0..vw as u32) as i32);
            let py = self.camera.y + (rng.random_range(0..vh as u32) as i32);
            if px >= 0
                && py >= 0
                && (px as usize) < self.state.wind.width
                && (py as usize) < self.state.wind.height
            {
                let (wx, wy) = self.state.wind.get_wind(px as usize, py as usize);
                let speed = self.state.wind.get_speed(px as usize, py as usize);
                if speed > 0.02 {
                    // Particle char hints at direction
                    let ch = if wx.abs() > wy.abs() {
                        if wx > 0.0 { '>' } else { '<' }
                    } else if wy.abs() > 0.01 {
                        if wy > 0.0 { 'v' } else { '^' }
                    } else {
                        '\u{00b7}'
                    };
                    // Color intensity by speed
                    let intensity = (speed * 2.0).min(1.0);
                    self.particles.push(Particle {
                        x: px as f64 + rng.random_range(-0.3..0.3),
                        y: py as f64 + rng.random_range(-0.3..0.3),
                        ch,
                        fg: Color(
                            (100.0 + 100.0 * intensity) as u8,
                            (180.0 + 50.0 * intensity) as u8,
                            255,
                        ),
                        life: 40,
                        max_life: 40,
                        dx: wx * 0.4,
                        dy: wy * 0.4,
                        emissive: false,
                    });
                }
            }
        }
    }

    /// Spawn activity particles from active processing buildings and villager activities.
    pub(super) fn spawn_activity_particles(&mut self) {
        let mut rng = rand::rng();
        let building_sources: Vec<(Recipe, f64, f64)> = self
            .world
            .query::<(&ProcessingBuilding, &Position)>()
            .iter()
            .filter(|(pb, _)| pb.worker_present)
            .map(|(pb, pos)| (pb.recipe, pos.x, pos.y))
            .collect();
        for (recipe, px, py) in building_sources {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            // Per-building-type particle signature
            let (spawn_rate, chars, fg, dx_range, dy_range, life_range, emissive) = match recipe {
                Recipe::WoodToPlanks => {
                    // Workshop: grey smoke, lazy drift
                    (
                        3u32,
                        &['.', '\u{00b0}', '\''][..],
                        Color(140, 130, 110),
                        (-0.05f64, 0.05f64),
                        (-0.15f64, -0.08f64),
                        (18u32, 28u32),
                        false,
                    )
                }
                Recipe::StoneToMasonry => {
                    // Smithy: orange sparks, fast rise, short life
                    (
                        2,
                        &['*', '\u{00b7}', '\''][..],
                        Color(255, 140, 40),
                        (-0.08, 0.08),
                        (-0.25, -0.10),
                        (10, 18),
                        true,
                    )
                }
                Recipe::FoodToGrain => {
                    // Granary: pale straw, minimal
                    (
                        4,
                        &['.', ','][..],
                        Color(180, 170, 120),
                        (-0.03, 0.03),
                        (-0.10, -0.05),
                        (12, 20),
                        false,
                    )
                }
                Recipe::GrainToBread => {
                    // Bakery: white steam plumes
                    (
                        2,
                        &['~', '\'', '.'][..],
                        Color(200, 200, 210),
                        (-0.06, 0.06),
                        (-0.12, -0.06),
                        (20, 35),
                        false,
                    )
                }
            };
            if rng.random_range(0..spawn_rate) == 0 {
                let ch = chars[rng.random_range(0..chars.len())];
                let life = rng.random_range(life_range.0..=life_range.1);
                self.particles.push(Particle {
                    x: px,
                    y: py - 1.0,
                    ch,
                    fg,
                    life,
                    max_life: life,
                    dx: rng.random_range(dx_range.0..dx_range.1),
                    dy: rng.random_range(dy_range.0..dy_range.1),
                    emissive,
                });
            }
        }

        // Spawn villager activity particles (construction dust, mining sparkle)
        let villager_activities: Vec<(BehaviorState, f64, f64)> = self
            .world
            .query::<(&Behavior, &Position)>()
            .iter()
            .map(|(b, pos)| (b.state, pos.x, pos.y))
            .collect();
        for (state, vx, vy) in villager_activities {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            match state {
                BehaviorState::Building {
                    target_x, target_y, ..
                } => {
                    // Construction: yellow-brown dust at build site
                    if rng.random_range(0..4) == 0 {
                        let chars = ['#', '.', '+'];
                        let ch = chars[rng.random_range(0..chars.len())];
                        let life = rng.random_range(6..=12);
                        self.particles.push(Particle {
                            x: target_x,
                            y: target_y,
                            ch,
                            fg: Color(220, 200, 100),
                            life,
                            max_life: life,
                            dx: rng.random_range(-0.15..0.15),
                            dy: rng.random_range(-0.10..0.10),
                            emissive: false,
                        });
                    }
                }
                BehaviorState::Gathering {
                    resource_type: ResourceType::Stone,
                    ..
                } => {
                    // Mining: white-blue sparkle
                    if rng.random_range(0..3) == 0 {
                        let chars = ['*', '\'', '.'];
                        let ch = chars[rng.random_range(0..chars.len())];
                        let life = rng.random_range(4..=8);
                        self.particles.push(Particle {
                            x: vx,
                            y: vy,
                            ch,
                            fg: Color(200, 200, 220),
                            life,
                            max_life: life,
                            dx: rng.random_range(-0.20..0.20),
                            dy: rng.random_range(-0.15..0.05),
                            emissive: false,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}
