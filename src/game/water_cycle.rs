use crate::simulation::WindField;
use crate::terrain_gen;

impl super::Game {
    /// Run rain, pipe_water stepping, wind-moisture advection, sediment transport,
    /// and seasonal vegetation/erosion updates for one tick.
    ///
    /// Extracted from step() — called once per sim tick inside the speed loop.
    pub(super) fn step_water_cycle(&mut self, should_rain: bool, veg_growth_mult: f64) {
        // Evolve curl noise wind field every 20 ticks.
        // Synoptic layer (bulk transport) changes very slowly (t * 0.0003),
        // mesoscale layer changes faster — updating every 20 ticks is enough
        // to capture local variation without disrupting moisture transport.
        if self.sim_config.wind_model == crate::simulation::WindModel::CurlNoise
            && self.tick % 20 == 0
        {
            self.wind
                .evolve_curl_noise(&self.heights, self.tick as f64, self.terrain_config.seed);
        }

        let is_advection_tick = self.tick % 3 == 0;

        // Rain mode determines how moisture enters the system.
        // The rest of the water cycle (pipe_water, sediment, vegetation) runs
        // regardless of rain mode — only the moisture SOURCE changes.
        match self.sim_config.rain_mode {
            crate::simulation::RainMode::WindDriven => {
                // Wind-driven water cycle: evaporation -> wind transport -> precipitation
                // ALL rain comes from wind-carried moisture (no uniform random rain).
                // Precipitation only on advection ticks to keep deposition in sync
                // with wind transport — prevents moisture raining out before it moves.
                self.moisture.update(
                    &mut self.pipe_water,
                    &mut self.vegetation,
                    &self.map,
                    &mut self.wind,
                    &self.heights,
                    is_advection_tick,
                );

                if is_advection_tick {
                    // Manual rain toggle ('r' in old mode): inject atmospheric moisture
                    if should_rain {
                        for v in self.wind.moisture_carried.iter_mut() {
                            *v = (*v + 0.01).min(1.0);
                        }
                    }
                    let (precip, evaporated) = self.wind.advect_moisture(
                        &self.heights,
                        &self.pipe_water.ocean_mask,
                        &self.moisture.moisture,
                    );
                    // Mass-conserving water cycle:
                    // - Subtract evaporated water from surface (conservation!)
                    // - Add precipitated water back to surface
                    for y in 0..self.map.height {
                        for x in 0..self.map.width {
                            let i = y * self.map.width + x;
                            // Remove evaporated water from surface
                            if evaporated[i] > 0.0001 {
                                let depth = self.pipe_water.get_depth(x, y);
                                let remove = evaporated[i].min(depth * 0.5);
                                self.pipe_water.add_water(x, y, -remove);
                            }
                            let p = precip[i];
                            if p > 0.0001 {
                                self.pipe_water.add_water(x, y, p * 0.5);
                            }
                            let excess = (self.wind.moisture_carried[i] - 0.8).max(0.0);
                            if excess > 0.001 {
                                self.pipe_water.add_water(x, y, excess * 0.1);
                                self.wind.moisture_carried[i] -= excess * 0.1;
                            }
                        }
                    }
                }
            }
            crate::simulation::RainMode::Uniform => {
                // Uniform rain: add moisture directly to soil, bypass wind transport.
                // Good for testing vegetation/groundwater without wind coupling.
                const UNIFORM_RAIN: f64 = 0.002;
                for i in 0..self.moisture.moisture.len() {
                    self.moisture.moisture[i] =
                        (self.moisture.moisture[i] + UNIFORM_RAIN).min(0.8);
                }
                // Still run moisture update for groundwater diffusion + vegetation
                self.moisture.update(
                    &mut self.pipe_water,
                    &mut self.vegetation,
                    &self.map,
                    &mut self.wind,
                    &self.heights,
                    false, // no wind precipitation
                );
            }
            crate::simulation::RainMode::Off => {
                // No rain — only passive decay, groundwater, vegetation
                self.moisture.update(
                    &mut self.pipe_water,
                    &mut self.vegetation,
                    &self.map,
                    &mut self.wind,
                    &self.heights,
                    false,
                );
            }
        }
        self.pipe_water.step(&self.heights, 0.1);

        // Sediment transport: run every 5 ticks (geological timescale)
        if self.tick % 5 == 0 {
            self.pipe_water.step_sediment(&mut self.heights);
        }

        // Seasonal vegetation decay (winter/autumn)
        self.vegetation.apply_season(veg_growth_mult);

        // rebuild tiles if erosion changed heights
        if self.sim_config.erosion_enabled {
            terrain_gen::rebuild_tiles(&mut self.map, &self.heights, &self.terrain_config);
        }
    }

    /// Handle seasonal transition effects: terrain overlays, wind recomputation,
    /// ice/thaw/floods, and notifications.
    ///
    /// Called from step() when the season changes.
    pub(super) fn handle_season_change(&mut self, prev_season: crate::simulation::Season) {
        use crate::ecs::FarmPlot;
        use crate::simulation::Season;

        self.dirty.mark_all(); // Season change affects all visible tiles
        let season_msg = match self.day_night.season {
            Season::Spring => "Spring has arrived — the ice thaws!",
            Season::Summer => "Summer heat — fire risk increases!",
            Season::Autumn => "Autumn harvest — gather while you can!",
            Season::Winter => "Winter descends — conserve resources!",
        };
        self.notify_milestone(season_msg);

        // --- Recompute wind field for new seasonal direction ---
        let wind_dir = WindField::seasonal_direction(self.day_night.season);
        self.wind = match self.sim_config.wind_model {
            crate::simulation::WindModel::CurlNoise => WindField::compute_curl_noise_field(
                &self.heights,
                self.map.width,
                self.map.height,
                wind_dir,
                self.wind.prevailing_strength,
                self.tick as f64,
                self.terrain_config.seed,
            ),
            crate::simulation::WindModel::Stam => WindField::compute_from_terrain(
                &self.heights,
                self.map.width,
                self.map.height,
                wind_dir,
                self.wind.prevailing_strength,
                Some(&self.chokepoint_map.scores),
            ),
        };

        // --- Seasonal terrain transitions ---
        // Revert previous season's overlays before applying new ones.
        match prev_season {
            Season::Winter => {
                // Thaw: revert Ice -> Water
                self.map.revert_ice();
            }
            Season::Spring => {
                // Floods recede: revert FloodWater -> base terrain
                let reverted = self.map.revert_flood_water();
                // Alluvial fertility bonus on tiles that were flooded
                for (x, y) in &reverted {
                    self.soil_fertility.add(*x, *y, 0.15);
                }
                if !reverted.is_empty() {
                    self.notify(format!(
                        "Floods recede — {} tiles enriched with alluvial soil",
                        reverted.len()
                    ));
                }
                self.flooded_tiles.clear();
                self.flood_start_tick = 0;
            }
            _ => {}
        }

        // Apply new season's effects
        match self.day_night.season {
            Season::Winter => {
                let frozen = self.map.apply_winter_ice();
                if frozen > 0 {
                    self.notify(format!(
                        "Rivers freeze! {} tiles of ice — wolves can cross!",
                        frozen
                    ));
                }
            }
            Season::Spring => {
                let flooded =
                    self.map
                        .apply_spring_floods(&self.river_mask, &self.heights, &self.soil);
                if !flooded.is_empty() {
                    // Destroy farms on flooded tiles
                    let mut destroyed_farms = 0u32;
                    let flood_set: std::collections::HashSet<(usize, usize)> =
                        flooded.iter().copied().collect();
                    let farm_entities: Vec<hecs::Entity> = self
                        .world
                        .query::<(hecs::Entity, &FarmPlot)>()
                        .iter()
                        .filter(|(_, f)| flood_set.contains(&(f.tile_x, f.tile_y)))
                        .map(|(e, _)| e)
                        .collect();
                    for entity in farm_entities {
                        let _ = self.world.despawn(entity);
                        destroyed_farms += 1;
                    }
                    let msg = if destroyed_farms > 0 {
                        format!(
                            "Spring floods! {} tiles flooded, {} farms destroyed",
                            flooded.len(),
                            destroyed_farms
                        )
                    } else {
                        format!("Spring floods! {} tiles flooded near rivers", flooded.len())
                    };
                    self.notify(msg);
                    self.flood_start_tick = self.tick;
                    self.flooded_tiles = flooded;
                }
            }
            _ => {}
        }
    }
}
