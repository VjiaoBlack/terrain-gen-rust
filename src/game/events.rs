use super::{Game, GameEvent, Milestone};
use crate::ecs::{
    self, Creature, FarmPlot, GarrisonBuilding, HutBuilding, ProcessingBuilding, Recipe, Species,
};
use crate::simulation::Season;
use crate::tilemap::Terrain;
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
                // Drought fires only when the settlement has a grain buffer proportional to
                // its population (grain ≥ pop*5). This prevents early-game instant death:
                // a drought with no reserves is just unavoidable starvation, not tension.
                // Once grain is established, drought is a meaningful resource challenge.
                //
                // Severity: 70% yield (30% reduction) for 150 ticks — enough to strain
                // an unprepared settlement but not auto-kill one with a healthy grain buffer.
                // Probability: 2% per 100-tick check; Summer = 12000 ticks (120 checks)
                // → ~91% chance of drought per eligible Summer, but only when grain is adequate.
                let villager_count = self
                    .world
                    .query::<&Creature>()
                    .iter()
                    .filter(|c| c.species == Species::Villager)
                    .count() as u32;
                let has_grain_buffer = self.resources.grain >= villager_count * 5;
                if has_grain_buffer
                    && !self.events.has_event_type("drought")
                    && rng.random_range(0u32..100) < 2
                {
                    self.events.active_events.push(GameEvent::Drought {
                        ticks_remaining: 150,
                    });
                    self.events
                        .event_log
                        .push("Drought! Water levels dropping.".to_string());
                    self.notify("Drought! Water levels dropping.".to_string());
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
                        .push("Bountiful rains! Moisture levels rising.".to_string());
                    self.notify("Bountiful rains! Moisture levels rising.".to_string());
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
                        ecs::spawn_villager_staggered(
                            &mut self.world,
                            cx as f64 + ox,
                            cy as f64 + oy,
                            self.tick,
                        );
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
    /// Milestones are purely narrative -- they do not affect threat_level or gameplay.
    pub(super) fn check_milestones(&mut self) {
        let check = |m: Milestone, milestones: &[Milestone]| !milestones.contains(&m);

        // --- Gather commonly needed state ---
        let year = self.day_night.year;
        let villager_count = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count() as u32;

        // === Explore Phase ===

        // FirstWoodGathered: stockpile has any wood
        if self.resources.wood > 0
            && check(Milestone::FirstWoodGathered, &self.difficulty.milestones)
        {
            self.difficulty
                .milestones
                .push(Milestone::FirstWoodGathered);
            self.notify_milestone("First timber hauled back to camp!");
        }

        // FirstStoneFound: settlement knows about at least one stone deposit
        if !self.knowledge.known_stone.is_empty()
            && check(Milestone::FirstStoneFound, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstStoneFound);
            self.notify_milestone("Stone deposit discovered!");
        }

        // FirstFarm: a completed FarmPlot exists
        if self.world.query::<&FarmPlot>().iter().count() > 0
            && check(Milestone::FirstFarm, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstFarm);
            self.notify_milestone("First farm planted -- food from the land!");
        }

        // FirstHut: a completed HutBuilding exists
        if self.world.query::<&HutBuilding>().iter().count() > 0
            && check(Milestone::FirstHut, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstHut);
            self.notify_milestone("First hut built -- settlers have shelter!");
        }

        // FirstWinterSurvived: year >= 1 (survived through first winter)
        if year >= 1
            && villager_count >= 1
            && check(Milestone::FirstWinterSurvived, &self.difficulty.milestones)
        {
            self.difficulty
                .milestones
                .push(Milestone::FirstWinterSurvived);
            self.notify_milestone("First winter survived!");
        }

        // === Expand Phase ===

        // PopulationTen
        if villager_count >= 10 && check(Milestone::PopulationTen, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::PopulationTen);
            self.notify_milestone("Population reached 10 -- a real village now!");
        }

        // FirstWorkshop: a ProcessingBuilding with WoodToPlanks recipe
        if self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::WoodToPlanks)
            && check(Milestone::FirstWorkshop, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstWorkshop);
            self.notify_milestone("Workshop built -- planks now available!");
        }

        // FirstSmith: a ProcessingBuilding with StoneToMasonry recipe
        if self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::StoneToMasonry)
            && check(Milestone::FirstSmith, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstSmith);
            self.notify_milestone("Smithy built -- stone becomes masonry!");
        }

        // FirstRoad: check every 100 ticks for any Road tile near settlement
        if self.tick.is_multiple_of(100) && check(Milestone::FirstRoad, &self.difficulty.milestones)
        {
            let (cx, cy) = self.settlement_center();
            let radius = 20i32;
            let mut found_road = false;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let x = cx as i32 + dx;
                    let y = cy as i32 + dy;
                    if x >= 0 && y >= 0 {
                        if let Some(&Terrain::Road) = self.map.get(x as usize, y as usize) {
                            found_road = true;
                            break;
                        }
                    }
                }
                if found_road {
                    break;
                }
            }
            if found_road {
                self.difficulty.milestones.push(Milestone::FirstRoad);
                self.notify_milestone("A footpath has worn into the earth!");
            }
        }

        // FiveBuildings: count completed buildings (excluding Stockpile and Road types)
        // We count FarmPlot + HutBuilding + ProcessingBuilding + GarrisonBuilding entities
        if check(Milestone::FiveBuildings, &self.difficulty.milestones) {
            let farm_count = self.world.query::<&FarmPlot>().iter().count();
            let hut_count = self.world.query::<&HutBuilding>().iter().count();
            let processing_count = self.world.query::<&ProcessingBuilding>().iter().count();
            let garrison_count = self.world.query::<&GarrisonBuilding>().iter().count();
            let total = farm_count + hut_count + processing_count + garrison_count;
            if total >= 5 {
                self.difficulty.milestones.push(Milestone::FiveBuildings);
                self.notify_milestone("Five structures standing -- the village takes shape!");
            }
        }

        // === Exploit Phase ===

        // PopulationTwentyFive
        if villager_count >= 25
            && check(Milestone::PopulationTwentyFive, &self.difficulty.milestones)
        {
            self.difficulty
                .milestones
                .push(Milestone::PopulationTwentyFive);
            self.notify_milestone("Population reached 25!");
        }

        // FirstGranary: a ProcessingBuilding with FoodToGrain recipe
        if self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::FoodToGrain)
            && check(Milestone::FirstGranary, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstGranary);
            self.notify_milestone("Granary built -- grain stores for winter!");
        }

        // FirstBakery: a ProcessingBuilding with GrainToBread recipe
        if self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::GrainToBread)
            && check(Milestone::FirstBakery, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstBakery);
            self.notify_milestone("Bakery built -- bread on the table!");
        }

        // FirstPlank: stockpile has any planks
        if self.resources.planks > 0 && check(Milestone::FirstPlank, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::FirstPlank);
            self.notify_milestone("First plank produced -- refined goods!");
        }

        // HundredFood: stockpile food >= 100
        if self.resources.food >= 100 && check(Milestone::HundredFood, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::HundredFood);
            self.notify_milestone("Food stores reached 100 -- plenty for now!");
        }

        // === Endure Phase ===

        // FirstGarrison: a completed GarrisonBuilding exists
        if self.world.query::<&GarrisonBuilding>().iter().count() > 0
            && check(Milestone::FirstGarrison, &self.difficulty.milestones)
        {
            self.difficulty.milestones.push(Milestone::FirstGarrison);
            self.notify_milestone("Garrison built -- the village can defend itself!");
        }

        // RaidSurvived: set by raid resolution logic via raid_survived_clean flag
        if self.raid_survived_clean && check(Milestone::RaidSurvived, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::RaidSurvived);
            self.notify_milestone("Raid repelled -- not a single soul lost!");
            self.raid_survived_clean = false;
        }

        // PopulationFifty
        if villager_count >= 50 && check(Milestone::PopulationFifty, &self.difficulty.milestones) {
            self.difficulty.milestones.push(Milestone::PopulationFifty);
            self.notify_milestone("Population reached 50 -- a thriving settlement!");
        }
    }
}
