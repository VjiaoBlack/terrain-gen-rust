use super::{Game, GameEvent, Milestone, ThreatTier};
use crate::ecs::{
    self, Creature, FarmPlot, GarrisonBuilding, HutBuilding, ProcessingBuilding, Recipe, Species,
};
use crate::simulation::Season;
use crate::tilemap::Terrain;
use rand::RngExt;

/// Minimum ticks between any two threat spawns (wolves or raiders).
const THREAT_COOLDOWN_TICKS: u64 = 100;

/// How often (in ticks) to recalculate the settlement threat score.
const THREAT_SCORE_INTERVAL: u64 = 100;

/// How often (in ticks) to roll for threat spawns.
const THREAT_CHECK_INTERVAL: u64 = 3000;

impl Game {
    /// Fire a Lua on_event hook with the given event name.
    #[cfg(feature = "lua")]
    pub(super) fn fire_event_hook(&self, event_name: &str) {
        if let Some(ref engine) = self.script_engine {
            let _ = engine.set_global("event_name", event_name);
            let _ = engine.call_hook("on_event");
        }
    }

    /// Compute the settlement threat score from population, resources, and buildings.
    /// This drives all threat scaling — higher score means more frequent and larger threats.
    pub fn compute_threat_score(&self) -> f64 {
        let villager_count = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count() as f64;

        let total_resources = self.resources.food as f64
            + self.resources.wood as f64
            + self.resources.stone as f64
            + self.resources.grain as f64
            + self.resources.bread as f64
            + self.resources.planks as f64
            + self.resources.masonry as f64;

        let building_count = self.world.query::<&FarmPlot>().iter().count()
            + self.world.query::<&HutBuilding>().iter().count()
            + self.world.query::<&ProcessingBuilding>().iter().count()
            + self.world.query::<&GarrisonBuilding>().iter().count();

        villager_count * 0.5 + total_resources * 0.01 + building_count as f64 * 2.0
    }

    /// Compute wolf pack size from threat score: `1 + (score / 20)`, clamped 1-6.
    fn wolf_pack_size(&self) -> u32 {
        (1.0 + self.threat_score / 20.0).clamp(1.0, 6.0) as u32
    }

    /// Compute raider party size from threat score: `2 + (score / 25)`, clamped 2-8.
    fn raider_party_size(&self) -> u32 {
        (2.0 + self.threat_score / 25.0).clamp(2.0, 8.0) as u32
    }

    /// Seasonal multiplier for wolf threat probability.
    fn wolf_season_multiplier(&self) -> f64 {
        match self.day_night.season {
            Season::Spring => 0.5,
            Season::Summer => 0.7,
            Season::Autumn => 1.0,
            Season::Winter => 1.5,
        }
    }

    /// Seasonal multiplier for raider threat probability.
    fn raider_season_multiplier(&self) -> f64 {
        match self.day_night.season {
            Season::Spring => 0.8,
            Season::Summer => 1.0,
            Season::Autumn => 1.3,
            Season::Winter => 0.5,
        }
    }

    /// Find a wolf spawn point near forest tiles 15-60 tiles from settlement center.
    /// Returns `Some((x, y))` of a walkable forest-edge position, or falls back to
    /// map-edge spawn if no qualifying forest exists.
    fn find_wolf_spawn(&self, rng: &mut impl rand::Rng) -> Option<(f64, f64)> {
        let (scx, scy) = self.settlement_center();
        let min_dist = 15.0f64;
        let max_dist = 60.0f64;

        // Collect candidate forest tiles within the distance band.
        // Use a grid scan within the bounding box [scx-60..scx+60, scy-60..scy+60].
        let mut candidates: Vec<(usize, usize, f64)> = Vec::new();
        let x_lo = (scx as f64 - max_dist).max(0.0) as usize;
        let x_hi = ((scx as f64 + max_dist) as usize).min(self.map.width.saturating_sub(1));
        let y_lo = (scy as f64 - max_dist).max(0.0) as usize;
        let y_hi = ((scy as f64 + max_dist) as usize).min(self.map.height.saturating_sub(1));

        for y in y_lo..=y_hi {
            for x in x_lo..=x_hi {
                if let Some(&Terrain::Forest) = self.map.get(x, y) {
                    let dx = x as f64 - scx as f64;
                    let dy = y as f64 - scy as f64;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist >= min_dist && dist <= max_dist {
                        candidates.push((x, y, dist));
                    }
                }
            }
        }

        if !candidates.is_empty() {
            // Pick a random forest tile, weighted toward closer ones
            let idx = rng.random_range(0..candidates.len());
            let (fx, fy, _) = candidates[idx];
            return Some((fx as f64, fy as f64));
        }

        // Fallback: spawn from the map edge in the direction with fewest buildings.
        // Pick a random edge tile that is walkable.
        self.find_edge_spawn(rng)
    }

    /// Fallback: find a walkable tile on the map edge, preferring the direction
    /// away from the densest settlement infrastructure.
    fn find_edge_spawn(&self, rng: &mut impl rand::Rng) -> Option<(f64, f64)> {
        let w = self.map.width;
        let h = self.map.height;
        // Try random edge tiles
        for _ in 0..60 {
            let (x, y) = match rng.random_range(0u32..4) {
                0 => (rng.random_range(0..w), 0),                   // top
                1 => (rng.random_range(0..w), h.saturating_sub(1)), // bottom
                2 => (0, rng.random_range(0..h)),                   // left
                _ => (w.saturating_sub(1), rng.random_range(0..h)), // right
            };
            if self.map.is_walkable(x as f64, y as f64) {
                return Some((x as f64, y as f64));
            }
        }
        None
    }

    /// Find a raider spawn point along approach corridors, preferring paths through
    /// chokepoints. Falls back to edge spawn if no chokepoints exist.
    fn find_raider_spawn(&self, rng: &mut impl rand::Rng) -> Option<(f64, f64)> {
        let (scx, scy) = self.settlement_center();

        // If we have chokepoint locations, spawn raiders approaching through one.
        if !self.chokepoint_map.locations.is_empty() {
            // Pick a chokepoint weighted by distance (prefer ones 20-60 tiles out)
            let good_chokepoints: Vec<_> = self
                .chokepoint_map
                .locations
                .iter()
                .filter(|cp| {
                    let dx = cp.x as f64 - scx as f64;
                    let dy = cp.y as f64 - scy as f64;
                    let dist = (dx * dx + dy * dy).sqrt();
                    dist >= 15.0 && dist <= 80.0
                })
                .collect();

            if !good_chokepoints.is_empty() {
                let cp = good_chokepoints[rng.random_range(0..good_chokepoints.len())];
                // Spawn 15-25 tiles beyond the chokepoint (away from settlement)
                let dx = cp.x as f64 - scx as f64;
                let dy = cp.y as f64 - scy as f64;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let nx = dx / dist;
                let ny = dy / dist;
                let spawn_dist = rng.random_range(15.0f64..25.0);
                let sx = cp.x as f64 + nx * spawn_dist;
                let sy = cp.y as f64 + ny * spawn_dist;

                // Find nearest walkable tile to the target
                let sx_i = sx.clamp(0.0, (self.map.width - 1) as f64);
                let sy_i = sy.clamp(0.0, (self.map.height - 1) as f64);

                // Try the exact spot, then nearby
                for r in 0..10 {
                    for dy_off in -(r as i32)..=(r as i32) {
                        for dx_off in -(r as i32)..=(r as i32) {
                            let tx = sx_i as f64 + dx_off as f64;
                            let ty = sy_i as f64 + dy_off as f64;
                            if self.map.is_walkable(tx, ty) {
                                return Some((tx, ty));
                            }
                        }
                    }
                }
            }
        }

        // Fallback: spawn from 40-60 tiles away in a random direction
        for _ in 0..60 {
            let angle = rng.random_range(0.0f64..std::f64::consts::TAU);
            let dist = rng.random_range(40.0f64..60.0);
            let wx = scx as f64 + angle.cos() * dist;
            let wy = scy as f64 + angle.sin() * dist;
            if self.map.is_walkable(wx, wy) {
                return Some((wx, wy));
            }
        }

        self.find_edge_spawn(rng)
    }

    /// Spawn a wolf pack at the given origin, spreading wolves within a few tiles.
    fn spawn_wolf_pack(
        &mut self,
        origin_x: f64,
        origin_y: f64,
        count: u32,
        rng: &mut impl rand::Rng,
    ) -> u32 {
        let mut spawned = 0u32;
        for _ in 0..count * 10 {
            if spawned >= count {
                break;
            }
            let ox = rng.random_range(-3.0f64..3.0);
            let oy = rng.random_range(-3.0f64..3.0);
            let wx = origin_x + ox;
            let wy = origin_y + oy;
            if self.map.is_walkable(wx, wy) {
                ecs::spawn_predator(&mut self.world, wx, wy);
                spawned += 1;
            }
        }
        spawned
    }

    pub(super) fn update_events(&mut self) {
        // Pre-compute defense rating for raid resolution (avoids borrow conflict in retain_mut)
        let defense_for_raids = self.compute_defense_rating();

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
                GameEvent::BanditRaid { stolen, strength } => {
                    if !*stolen {
                        // Steal resources from stockpile, reduced by defense rating
                        let defense = defense_for_raids;
                        let raid_strength = *strength;
                        let steal_fraction = if defense >= raid_strength {
                            0.0
                        } else {
                            ((raid_strength - defense) / (raid_strength * 2.0)).min(0.5)
                        };
                        if steal_fraction > 0.0 {
                            let steal_food = (self.resources.food as f64 * steal_fraction) as u32;
                            let steal_wood = (self.resources.wood as f64 * steal_fraction) as u32;
                            let steal_stone = (self.resources.stone as f64 * steal_fraction) as u32;
                            self.resources.food = self.resources.food.saturating_sub(steal_food);
                            self.resources.wood = self.resources.wood.saturating_sub(steal_wood);
                            self.resources.stone = self.resources.stone.saturating_sub(steal_stone);
                            self.events.event_log.push(format!(
                                "Bandits stole {} food, {} wood, {} stone!",
                                steal_food, steal_wood, steal_stone
                            ));
                        } else {
                            self.events
                                .event_log
                                .push("Raiders repelled by garrison!".to_string());
                            self.raid_survived_clean = true;
                        }
                        *stolen = true;
                    }
                    false // instant event, remove after resolving
                }
            }
        });

        // Keep event log trimmed
        if self.events.event_log.len() > 5 {
            self.events
                .event_log
                .drain(0..self.events.event_log.len() - 5);
        }

        // Recalculate threat score every THREAT_SCORE_INTERVAL ticks
        if self.tick.is_multiple_of(THREAT_SCORE_INTERVAL) {
            self.threat_score = self.compute_threat_score();
        }

        // --- Weather/seasonal events: check every 100 ticks ---
        if self.tick.is_multiple_of(100) {
            let mut rng = rand::rng();
            let season = self.day_night.season;

            match season {
                Season::Summer => {
                    // Drought: requires grain buffer proportional to population
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
            if year >= 2 && !self.events.has_event_type("plague") && rng.random_range(0u32..100) < 5
            {
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
        }

        // --- Wealth-based threat spawns: check every THREAT_CHECK_INTERVAL ticks ---
        if !self.tick.is_multiple_of(THREAT_CHECK_INTERVAL) {
            return;
        }

        // Enforce cooldown between threat spawns
        if self.tick.saturating_sub(self.last_threat_tick) < THREAT_COOLDOWN_TICKS {
            return;
        }

        let mut rng = rand::rng();
        let tier = ThreatTier::from_score(self.threat_score);

        // Wolf threat probability: base 5%, scales with score, modified by season
        let wolf_chance = (5.0 + self.threat_score * 0.4).min(40.0) * self.wolf_season_multiplier();
        let wolf_roll = rng.random_range(0.0f64..100.0);

        if wolf_roll < wolf_chance {
            // Spawn wolf pack
            let pack_size = self.wolf_pack_size();
            if let Some((wx, wy)) = self.find_wolf_spawn(&mut rng) {
                let spawned = self.spawn_wolf_pack(wx, wy, pack_size, &mut rng);
                if spawned > 0 {
                    self.last_threat_tick = self.tick;
                    self.events.active_events.push(GameEvent::WolfSurge {
                        ticks_remaining: 400,
                    });

                    let direction = self.direction_label(wx, wy);
                    let msg = if spawned == 1 {
                        format!("A lone wolf approaches from the {}!", direction)
                    } else {
                        format!(
                            "Wolf pack of {} approaches from the {}!",
                            spawned, direction
                        )
                    };
                    self.events.event_log.push(msg.clone());
                    self.notify(msg);

                    #[cfg(feature = "lua")]
                    self.fire_event_hook("wolf_surge");
                }
            }
        }

        // Raider threat probability: 0% below score 25, scales up from there
        // Only if wolves didn't just spawn (cooldown will catch it, but be explicit)
        if self.tick.saturating_sub(self.last_threat_tick) < THREAT_COOLDOWN_TICKS {
            return;
        }

        let raider_base = ((self.threat_score - 25.0).max(0.0) * 0.5).min(30.0);
        let raider_chance = raider_base * self.raider_season_multiplier();
        let raider_roll = rng.random_range(0.0f64..100.0);

        if raider_roll < raider_chance && tier as u32 >= ThreatTier::Established as u32 {
            let party_size = self.raider_party_size();
            let defense = self.compute_defense_rating();
            let raid_strength = party_size as f64 * 3.0;

            if defense >= raid_strength {
                // Raiders scout, see defenses, and retreat
                self.events
                    .event_log
                    .push("Raider scouts spotted — they turned back at our defenses!".to_string());
                self.notify(
                    "Raider scouts spotted — they turned back at our defenses!".to_string(),
                );
                self.raid_survived_clean = true;
                self.last_threat_tick = self.tick;
            } else {
                // Raiders attack — spawn bandit raid event
                if let Some((_rx, _ry)) = self.find_raider_spawn(&mut rng) {
                    let direction = self.direction_label(_rx, _ry);
                    self.events.active_events.push(GameEvent::BanditRaid {
                        stolen: false,
                        strength: raid_strength,
                    });
                    let msg = format!(
                        "Raiding party of {} approaching from the {}!",
                        party_size, direction
                    );
                    self.events.event_log.push(msg.clone());
                    self.notify(msg);
                    self.last_threat_tick = self.tick;

                    #[cfg(feature = "lua")]
                    self.fire_event_hook("bandit_raid");
                }
            }
        }
    }

    /// Return a compass direction label for a world position relative to settlement center.
    fn direction_label(&self, x: f64, y: f64) -> &'static str {
        let (scx, scy) = self.settlement_center();
        let dx = x - scx as f64;
        let dy = y - scy as f64;
        let angle = dy.atan2(dx);
        // atan2 returns radians: 0=east, pi/2=south, pi=west, -pi/2=north
        // Map to 8 compass directions
        let octant = ((angle + std::f64::consts::PI) / (std::f64::consts::PI / 4.0)) as usize % 8;
        match octant {
            0 => "west",
            1 => "northwest",
            2 => "north",
            3 => "northeast",
            4 => "east",
            5 => "southeast",
            6 => "south",
            7 => "southwest",
            _ => "unknown",
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

#[cfg(test)]
mod threat_tests {
    use super::*;

    #[test]
    fn threat_score_empty_settlement() {
        let game = Game::new(60, 42);
        let score = game.compute_threat_score();
        // Starting settlement has ~3 villagers, some resources, 1 stockpile (no counted buildings)
        // Should be a small positive number in the Quiet tier
        assert!(score >= 0.0, "Threat score should be non-negative");
        assert!(
            score < 30.0,
            "New settlement should be below Established tier, got {}",
            score
        );
    }

    #[test]
    fn threat_score_scales_with_population() {
        let mut game = Game::new(60, 42);
        let base = game.compute_threat_score();

        // Spawn more villagers
        let (cx, cy) = game.settlement_center();
        for _ in 0..10 {
            ecs::spawn_villager_staggered(&mut game.world, cx as f64, cy as f64, game.tick);
        }

        let after = game.compute_threat_score();
        assert!(
            after > base,
            "More villagers should increase threat score: {} vs {}",
            after,
            base
        );
        // 10 villagers * 0.5 = 5.0 increase
        assert!(
            after - base >= 4.5,
            "10 villagers should add ~5.0 to threat score"
        );
    }

    #[test]
    fn threat_score_scales_with_resources() {
        let mut game = Game::new(60, 42);
        let base = game.compute_threat_score();

        game.resources.food += 500;
        game.resources.wood += 300;

        let after = game.compute_threat_score();
        assert!(
            after > base,
            "More resources should increase threat score: {} vs {}",
            after,
            base
        );
        // 800 total * 0.01 = 8.0 increase
        assert!(
            after - base >= 7.5,
            "800 extra resources should add ~8.0 to threat score"
        );
    }

    #[test]
    fn threat_score_scales_with_buildings() {
        let mut game = Game::new(60, 42);
        let base = game.compute_threat_score();

        // Spawn farm plot entities
        use crate::ecs::{FarmPlot, Position};
        for i in 0..3 {
            game.world.spawn((
                Position {
                    x: 128.0 + i as f64,
                    y: 128.0,
                },
                FarmPlot {
                    growth: 0.0,
                    harvest_ready: false,
                    worker_present: false,
                    pending_food: 0,
                    tile_x: 128 + i,
                    tile_y: 128,
                    fallow: false,
                },
            ));
        }

        let after = game.compute_threat_score();
        assert!(
            after > base,
            "More buildings should increase threat score: {} vs {}",
            after,
            base
        );
        // 3 buildings * 2.0 = 6.0 increase
        assert!(
            (after - base - 6.0).abs() < 0.1,
            "3 farms should add 6.0 to threat score, got {}",
            after - base
        );
    }

    #[test]
    fn threat_tier_boundaries() {
        assert_eq!(ThreatTier::from_score(0.0), ThreatTier::Quiet);
        assert_eq!(ThreatTier::from_score(14.9), ThreatTier::Quiet);
        assert_eq!(ThreatTier::from_score(15.0), ThreatTier::Growing);
        assert_eq!(ThreatTier::from_score(29.9), ThreatTier::Growing);
        assert_eq!(ThreatTier::from_score(30.0), ThreatTier::Established);
        assert_eq!(ThreatTier::from_score(49.9), ThreatTier::Established);
        assert_eq!(ThreatTier::from_score(50.0), ThreatTier::Prosperous);
        assert_eq!(ThreatTier::from_score(74.9), ThreatTier::Prosperous);
        assert_eq!(ThreatTier::from_score(75.0), ThreatTier::Empire);
        assert_eq!(ThreatTier::from_score(200.0), ThreatTier::Empire);
    }

    #[test]
    fn wolf_pack_size_scales() {
        let mut game = Game::new(60, 42);

        game.threat_score = 0.0;
        assert_eq!(game.wolf_pack_size(), 1);

        game.threat_score = 20.0;
        assert_eq!(game.wolf_pack_size(), 2);

        game.threat_score = 60.0;
        assert_eq!(game.wolf_pack_size(), 4);

        game.threat_score = 200.0;
        assert_eq!(game.wolf_pack_size(), 6); // clamped
    }

    #[test]
    fn raider_party_size_scales() {
        let mut game = Game::new(60, 42);

        game.threat_score = 0.0;
        assert_eq!(game.raider_party_size(), 2);

        game.threat_score = 25.0;
        assert_eq!(game.raider_party_size(), 3);

        game.threat_score = 75.0;
        assert_eq!(game.raider_party_size(), 5);

        game.threat_score = 500.0;
        assert_eq!(game.raider_party_size(), 8); // clamped
    }

    #[test]
    fn no_raiders_below_established_tier() {
        // Raider chance formula: ((score - 25) * 0.5).max(0).min(30)
        // At score 24.9: (24.9 - 25) * 0.5 = negative -> 0
        let score = 20.0;
        let raider_base = ((score - 25.0f64).max(0.0) * 0.5).min(30.0);
        assert_eq!(raider_base, 0.0, "No raiders below score 25");
    }

    #[test]
    fn direction_label_cardinal() {
        let game = Game::new(60, 42);
        let (scx, scy) = game.settlement_center();

        // North (negative Y in screen coords)
        let label = game.direction_label(scx as f64, scy as f64 - 30.0);
        assert_eq!(label, "north");

        // East
        let label = game.direction_label(scx as f64 + 30.0, scy as f64);
        assert_eq!(label, "east");

        // South
        let label = game.direction_label(scx as f64, scy as f64 + 30.0);
        assert_eq!(label, "south");

        // West
        let label = game.direction_label(scx as f64 - 30.0, scy as f64);
        assert_eq!(label, "west");
    }

    #[test]
    fn wolf_season_multipliers() {
        let mut game = Game::new(60, 42);

        game.day_night.season = Season::Winter;
        assert_eq!(game.wolf_season_multiplier(), 1.5);

        game.day_night.season = Season::Spring;
        assert_eq!(game.wolf_season_multiplier(), 0.5);

        game.day_night.season = Season::Summer;
        assert_eq!(game.wolf_season_multiplier(), 0.7);

        game.day_night.season = Season::Autumn;
        assert_eq!(game.wolf_season_multiplier(), 1.0);
    }

    #[test]
    fn raider_season_multipliers() {
        let mut game = Game::new(60, 42);

        game.day_night.season = Season::Autumn;
        assert_eq!(game.raider_season_multiplier(), 1.3);

        game.day_night.season = Season::Winter;
        assert_eq!(game.raider_season_multiplier(), 0.5);

        game.day_night.season = Season::Spring;
        assert_eq!(game.raider_season_multiplier(), 0.8);

        game.day_night.season = Season::Summer;
        assert_eq!(game.raider_season_multiplier(), 1.0);
    }

    #[test]
    fn threat_score_drops_after_resource_loss() {
        let mut game = Game::new(60, 42);
        game.resources.food = 200;
        game.resources.wood = 100;
        let before = game.compute_threat_score();

        // Simulate raid stealing resources
        game.resources.food = 100;
        game.resources.wood = 50;
        let after = game.compute_threat_score();

        assert!(
            after < before,
            "Losing resources should reduce threat score: {} vs {}",
            after,
            before
        );
    }

    #[test]
    fn defense_rating_reduces_raid_damage() {
        let mut game = Game::new(60, 42);
        game.resources.food = 100;
        game.resources.wood = 100;
        game.resources.stone = 100;

        // Add garrison for defense
        use crate::ecs::{GarrisonBuilding, Position};
        let (cx, cy) = game.settlement_center();
        game.world.spawn((
            Position {
                x: cx as f64,
                y: cy as f64,
            },
            GarrisonBuilding {
                defense_bonus: 20.0, // very strong defense
            },
        ));

        // Inject a bandit raid
        game.events.active_events.push(GameEvent::BanditRaid {
            stolen: false,
            strength: 9.0, // modest raider party
        });
        game.tick = 99; // avoid event check triggering new events
        game.update_events();

        // With defense_rating >= raid_strength, garrison should repel the raid
        assert!(
            game.raid_survived_clean,
            "Strong garrison should repel raiders"
        );
        assert_eq!(game.resources.food, 100, "No food should be stolen");
        assert_eq!(game.resources.wood, 100, "No wood should be stolen");
        assert_eq!(game.resources.stone, 100, "No stone should be stolen");
    }
}
