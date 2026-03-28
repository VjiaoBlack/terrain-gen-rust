use rand::RngExt;
use crate::ecs::{self, Creature, HutBuilding, Species};
use crate::simulation::Season;
use super::{Game, GameEvent};

impl Game {
    pub(super) fn update_events(&mut self) {
        // Tick down duration-based events, remove expired
        self.events.active_events.retain_mut(|event| {
            match event {
                GameEvent::Drought { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events.event_log.push("Drought has ended.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::BountifulHarvest { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events.event_log.push("Bountiful harvest season ends.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::WolfSurge { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events.event_log.push("Wolf surge subsides.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::Migration { .. } => false, // instant, remove after spawning
            }
        });

        // Keep event log trimmed
        if self.events.event_log.len() > 5 {
            self.events.event_log.drain(0..self.events.event_log.len() - 5);
        }

        // Check for new events every 100 ticks
        if self.tick % 100 != 0 { return; }

        let mut rng = rand::rng();
        let season = self.day_night.season;

        match season {
            Season::Summer => {
                if !self.events.has_event_type("drought") && rng.random_range(0u32..100) < 15 {
                    self.events.active_events.push(GameEvent::Drought { ticks_remaining: 300 });
                    self.events.event_log.push("Drought! Farm yields halved.".to_string());
                    self.notify("Drought! Farm yields halved.".to_string());
                }
            }
            Season::Autumn => {
                if !self.events.has_event_type("harvest") && rng.random_range(0u32..100) < 20 {
                    self.events.active_events.push(GameEvent::BountifulHarvest { ticks_remaining: 200 });
                    self.events.event_log.push("Bountiful harvest! Farm yields doubled.".to_string());
                    self.notify("Bountiful harvest! Farm yields doubled.".to_string());
                }
            }
            Season::Spring => {
                // Migration: new villagers arrive if food surplus and housing available
                let villager_count = self.world.query::<&Creature>().iter()
                    .filter(|c| c.species == Species::Villager).count() as u32;
                let hut_capacity: u32 = self.world.query::<&HutBuilding>().iter()
                    .map(|h| h.capacity).sum();
                let has_housing = hut_capacity > villager_count;
                if has_housing && self.resources.food > 30 && rng.random_range(0u32..100) < 20 {
                    let count = rng.random_range(1u32..4);
                    let (cx, cy) = self.settlement_center();
                    for _ in 0..count {
                        let ox = rng.random_range(-3i32..4) as f64;
                        let oy = rng.random_range(-3i32..4) as f64;
                        ecs::spawn_villager(&mut self.world, cx as f64 + ox, cy as f64 + oy);
                    }
                    self.events.event_log.push(format!("{} migrants arrived!", count));
                    self.notify(format!("{} migrants arrived!", count));
                }
            }
            Season::Winter => {
                if !self.events.has_event_type("wolf_surge") && rng.random_range(0u32..100) < 25 {
                    self.events.active_events.push(GameEvent::WolfSurge { ticks_remaining: 400 });
                    self.events.event_log.push("Wolf surge! Pack activity increases.".to_string());
                    self.notify("Wolf surge! Pack activity increases.".to_string());
                }
            }
        }
    }
}
