use super::{Game, GameEvent, Milestone};
use crate::ecs::{self, Creature, HutBuilding, ProcessingBuilding, Recipe, Species};
use crate::simulation::Season;
use rand::RngExt;

impl Game {
    /// Fire a Lua on_event hook with the given event name.
    #[cfg(feature = "lua")]
    pub(super) fn fire_event_hook(&self, event_name: &str) {
        if let Some(ref engine) = self.script_engine {
            let _ = engine.set_global("event_name", event_name);
            let _ = engine.call_hook("on_event");
        }
    }

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
                        self.events
                            .event_log
                            .push("Bountiful harvest season ends.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::WolfSurge { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events
                            .event_log
                            .push("Wolf surge subsides.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::Migration { .. } => false, // instant, remove after spawning
                GameEvent::Plague {
                    ticks_remaining, ..
                } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events
                            .event_log
                            .push("The plague has passed.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::Blizzard { ticks_remaining } => {
                    *ticks_remaining = ticks_remaining.saturating_sub(1);
                    if *ticks_remaining == 0 {
                        self.events
                            .event_log
                            .push("The blizzard has ended.".to_string());
                        return false;
                    }
                    true
                }
                GameEvent::BanditRaid { stolen } => {
                    if !*stolen {
                        // Steal resources from stockpile
                        let steal_food = self.resources.food / 4;
                        let steal_wood = self.resources.wood / 4;
                        let steal_stone = self.resources.stone / 4;
                        self.resources.food = self.resources.food.saturating_sub(steal_food);
                        self.resources.wood = self.resources.wood.saturating_sub(steal_wood);
                        self.resources.stone = self.resources.stone.saturating_sub(steal_stone);
                        self.events.event_log.push(format!(
                            "Bandits stole {} food, {} wood, {} stone!",
                            steal_food, steal_wood, steal_stone
                        ));
                        *stolen = true;
                    }
                    false // instant event, remove after stealing
                }
            }
        });

        // Keep event log trimmed
        if self.events.event_log.len() > 5 {
            self.events
                .event_log
                .drain(0..self.events.event_log.len() - 5);
        }

        // Check for new events every 100 ticks
        if !self.tick.is_multiple_of(100) {
            return;
        }

        let mut rng = rand::rng();
        let season = self.day_night.season;

        match season {
            Season::Summer => {
                if !self.events.has_event_type("drought") && rng.random_range(0u32..100) < 15 {
                    self.events.active_events.push(GameEvent::Drought {
                        ticks_remaining: 300,
                    });
                    self.events
                        .event_log
                        .push("Drought! Farm yields halved.".to_string());
                    self.notify("Drought! Farm yields halved.".to_string());
                    #[cfg(feature = "lua")]
                    self.fire_event_hook("drought");
                }
            }
            Season::Autumn => {
                if !self.events.has_event_type("harvest") && rng.random_range(0u32..100) < 20 {
                    self.events.active_events.push(GameEvent::BountifulHarvest {
                        ticks_remaining: 200,
                    });
                    self.events
                        .event_log
                        .push("Bountiful harvest! Farm yields doubled.".to_string());
                    self.notify("Bountiful harvest! Farm yields doubled.".to_string());
                    #[cfg(feature = "lua")]
                    self.fire_event_hook("harvest");
                }
            }
            Season::Spring => {
                // Migration: new villagers arrive if food surplus and housing available
                let villager_count = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Villager)
                    .count() as u32;
                let hut_capacity: u32 = self
                    .world
                    .query::<&HutBuilding>()
                    .iter()
                    .map(|h| h.capacity)
                    .sum();
                let has_housing = hut_capacity > villager_count;
                if has_housing && self.resources.food > 30 && rng.random_range(0u32..100) < 20 {
                    let count = rng.random_range(1u32..4);
                    let (cx, cy) = self.settlement_center();
                    for _ in 0..count {
                        let ox = rng.random_range(-3i32..4) as f64;
                        let oy = rng.random_range(-3i32..4) as f64;
                        ecs::spawn_villager(&mut self.world, cx as f64 + ox, cy as f64 + oy);
                    }
                    self.events
                        .event_log
                        .push(format!("{} migrants arrived!", count));
                    self.notify(format!("{} migrants arrived!", count));
                    #[cfg(feature = "lua")]
                    self.fire_event_hook("migration");
                }
            }
            Season::Winter => {
                if !self.events.has_event_type("wolf_surge") && rng.random_range(0u32..100) < 25 {
                    self.events.active_events.push(GameEvent::WolfSurge {
                        ticks_remaining: 400,
                    });
                    self.events
                        .event_log
                        .push("Wolf surge! Pack activity increases.".to_string());
                    self.notify("Wolf surge! Pack activity increases.".to_string());

                    // Spawn 3-5 predators in a ring 20-38 tiles from settlement center
                    let villager_pos: Vec<(f64, f64)> = self
                        .world
                        .query::<(&crate::ecs::Position, &Creature)>()
                        .iter()
                        .filter(|(_, c)| c.species == Species::Villager)
                        .map(|(p, _)| (p.x, p.y))
                        .collect();
                    if !villager_pos.is_empty() {
                        let cx = villager_pos.iter().map(|p| p.0).sum::<f64>()
                            / villager_pos.len() as f64;
                        let cy = villager_pos.iter().map(|p| p.1).sum::<f64>()
                            / villager_pos.len() as f64;
                        let wolf_count = rng.random_range(3u32..=5);
                        let mut spawned = 0u32;
                        for attempt in 0..60u32 {
                            let angle = (attempt as f64) * std::f64::consts::TAU / 60.0
                                + rng.random_range(0.0f64..0.5);
                            let dist = rng.random_range(20.0f64..38.0);
                            let wx = cx + angle.cos() * dist;
                            let wy = cy + angle.sin() * dist;
                            if self.map.is_walkable(wx, wy) {
                                ecs::spawn_predator(&mut self.world, wx, wy);
                                spawned += 1;
                                if spawned >= wolf_count {
                                    break;
                                }
                            }
                        }
                        if spawned > 0 {
                            self.notify(format!("{} wolves approach!", spawned));
                        }
                    }

                    #[cfg(feature = "lua")]
                    self.fire_event_hook("wolf_surge");

                    // Spawn wolves scaled to settlement size: small settlements get 1-2 wolves,
                    // large settlements get up to 4. 4 wolves vs pop=8 was an instant wipe.
                    let (scx, scy) = self.settlement_center();
                    let villager_count = self
                        .world
                        .query::<&ecs::Creature>()
                        .iter()
                        .filter(|c| c.species == ecs::Species::Villager)
                        .count() as u32;
                    let max_wolves = (villager_count / 5 + 1).clamp(1, 4);
                    let wolf_count = rng.random_range(1u32..=max_wolves);
                    let mut spawned = 0u32;
                    for _ in 0..60 {
                        if spawned >= wolf_count {
                            break;
                        }
                        let angle = rng.random_range(0.0f64..std::f64::consts::TAU);
                        let dist = rng.random_range(20.0f64..35.0);
                        let wx = scx as f64 + angle.cos() * dist;
                        let wy = scy as f64 + angle.sin() * dist;
                        if self.map.is_walkable(wx, wy) {
                            ecs::spawn_predator(&mut self.world, wx, wy);
                            spawned += 1;
                        }
                    }
                    if spawned > 0 {
                        self.events
                            .event_log
                            .push(format!("{} wolves approach!", spawned));
                    }
                }
                // Blizzard: winter-only, halves movement speed
                if !self.events.has_event_type("blizzard") && rng.random_range(0u32..100) < 10 {
                    self.events.active_events.push(GameEvent::Blizzard {
                        ticks_remaining: 200,
                    });
                    self.events
                        .event_log
                        .push("Blizzard! Movement slowed.".to_string());
                    self.notify("Blizzard! Movement slowed.".to_string());
                    #[cfg(feature = "lua")]
                    self.fire_event_hook("blizzard");
                }
            }
        }

        let year = self.day_night.year;

        // Plague: year 2+, kills 1-2 villagers unless Bakery exists
        if year >= 2 && !self.events.has_event_type("plague") && rng.random_range(0u32..100) < 5 {
            let has_bakery = self
                .world
                .query::<&ProcessingBuilding>()
                .iter()
                .any(|pb| pb.recipe == Recipe::GrainToBread);
            if !has_bakery {
                let kills = rng.random_range(1u32..=2);
                self.events.active_events.push(GameEvent::Plague {
                    ticks_remaining: 300,
                    kills_remaining: kills,
                });
                self.events
                    .event_log
                    .push(format!("Plague strikes! {} villagers at risk.", kills));
                self.notify(format!("Plague strikes! {} villagers at risk.", kills));
                #[cfg(feature = "lua")]
                self.fire_event_hook("plague");
            }
        }

        // Bandit raid: year 3+, steals 25% of resources
        if year >= 3
            && !self.events.has_event_type("bandit_raid")
            && rng.random_range(0u32..100) < 8
        {
            self.events
                .active_events
                .push(GameEvent::BanditRaid { stolen: false });
            self.events
                .event_log
                .push("Bandit raid! Resources stolen!".to_string());
            self.notify("Bandit raid incoming!".to_string());
            #[cfg(feature = "lua")]
            self.fire_event_hook("bandit_raid");
        }
    }

    /// Check and award milestones based on current game state.
    pub(super) fn check_milestones(&mut self) {
        let year = self.day_night.year;
        let villager_count = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count() as u32;
        let has_garrison = self
            .world
            .query::<&crate::ecs::GarrisonBuilding>()
            .iter()
            .count()
            > 0;

        let check = |m: Milestone, milestones: &[Milestone]| !milestones.contains(&m);

        if year >= 1 && check(Milestone::FirstWinter, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::FirstWinter);
            self.notify("Milestone: Survived first winter!".to_string());
            self.difficulty.threat_level += 0.5;
        }
        if villager_count >= 10 && check(Milestone::TenVillagers, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::TenVillagers);
            self.notify("Milestone: 10 villagers!".to_string());
            self.difficulty.threat_level += 0.5;
        }
        if has_garrison && check(Milestone::FirstGarrison, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::FirstGarrison);
            self.notify("Milestone: First garrison built!".to_string());
            self.difficulty.threat_level += 0.5;
        }
        if year >= 5 && check(Milestone::FiveYears, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::FiveYears);
            self.notify("Milestone: 5 years survived!".to_string());
            self.difficulty.threat_level += 1.0;
        }
        if villager_count >= 20 && check(Milestone::TwentyVillagers, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::TwentyVillagers);
            self.notify("Milestone: 20 villagers!".to_string());
            self.difficulty.threat_level += 1.0;
        }
    }
}
