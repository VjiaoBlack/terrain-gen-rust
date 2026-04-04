use super::{
    Behavior, BehaviorState, CELL_ASPECT, Color, FarmPlot, FoodSource, PANEL_WIDTH,
    ProcessingBuilding, Renderer, Sprite, Stockpile, StoneDeposit,
};
use crate::ecs::{self, Creature, Den, Position, Species};

impl super::super::Game {
    pub(in super::super) fn draw_query_cursor(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;
        let aspect = CELL_ASPECT;
        let panel_w = PANEL_WIDTH as i32;

        let sx = (self.query_cx - self.camera.x) * aspect + panel_w;
        let sy = self.query_cy - self.camera.y;

        // Draw cursor bracket across aspect-width cells
        if sy >= 0 && (sy as u16) < h.saturating_sub(status_h) {
            for dx in 0..aspect {
                let cx = sx + dx;
                if cx >= panel_w && (cx as u16) < w {
                    // Draw a highlight — bright magenta border
                    let cell = renderer.get_cell(cx as u16, sy as u16);
                    let ch = cell.map(|c| c.ch).unwrap_or(' ');
                    renderer.draw(
                        cx as u16,
                        sy as u16,
                        ch,
                        Color(255, 255, 255),
                        Some(Color(180, 0, 180)),
                    );
                }
            }
        }
    }

    pub(in super::super) fn draw_query_panel(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();
        let status_h = 1u16;

        // Gather info about the tile and any entities at cursor
        let wx = self.query_cx;
        let wy = self.query_cy;

        let mut lines: Vec<String> = Vec::new();

        // Tile info
        if wx >= 0 && wy >= 0 {
            let ux = wx as usize;
            let uy = wy as usize;
            if let Some(terrain) = self.map.get(ux, uy) {
                lines.push(format!("({},{}) {:?}", wx, wy, terrain));
                if ux < self.map.width && uy < self.map.height {
                    let height = self.heights[uy * self.map.width + ux];
                    lines.push(format!("height: {:.3}", height));
                }
                let water_depth = if ux < self.pipe_water.width && uy < self.pipe_water.height {
                    self.pipe_water.get_depth(ux, uy)
                } else {
                    0.0
                };
                if water_depth > 0.0001 {
                    lines.push(format!("water: {:.4}", water_depth));
                }
                let moisture = if ux < self.moisture.width && uy < self.moisture.height {
                    self.moisture.get(ux, uy)
                } else {
                    0.0
                };
                if moisture > 0.01 {
                    lines.push(format!("moisture: {:.2}", moisture));
                }
                let veg = if ux < self.vegetation.width && uy < self.vegetation.height {
                    self.vegetation.get(ux, uy)
                } else {
                    0.0
                };
                if veg > 0.01 {
                    lines.push(format!("vegetation: {:.2}", veg));
                }
                let inf = if ux < self.influence.width && uy < self.influence.height {
                    self.influence.get(ux, uy)
                } else {
                    0.0
                };
                if inf > 0.01 {
                    lines.push(format!("influence: {:.2}", inf));
                }
                // Pipeline data for biome debugging
                let idx = uy * self.map.width + ux;
                if idx < self.pipeline_temperature.len() {
                    lines.push(format!("temp: {:.2}", self.pipeline_temperature[idx]));
                }
                if idx < self.pipeline_slope.len() {
                    lines.push(format!("slope: {:.3}", self.pipeline_slope[idx]));
                }
                if idx < self.pipeline_moisture.len() {
                    lines.push(format!("p_moist: {:.2}", self.pipeline_moisture[idx]));
                }
                let avg_m = self.moisture.get_avg(ux, uy);
                lines.push(format!("avg_moist: {:.2}", avg_m));
                // Show what biome this tile WOULD be if reclassified now
                if idx < self.pipeline_temperature.len() && idx < self.pipeline_slope.len() {
                    let would_be = crate::terrain_pipeline::classify_biome(
                        self.heights[idx],
                        self.pipeline_temperature[idx],
                        avg_m, // use average moisture instead of frozen pipeline moisture
                        self.pipeline_slope[idx],
                        self.terrain_config.water_level,
                    );
                    if let Some(current) = self.map.get(ux, uy) {
                        if *current != would_be {
                            lines.push(format!("→ would be: {:?}", would_be));
                        }
                    }
                }
                if idx < self.soil.len() {
                    lines.push(format!("soil: {:?}", self.soil[idx]));
                }
                lines.push(format!("fertility: {:.2}", self.soil_fertility.get(ux, uy)));
                // River proximity
                if idx < self.river_mask.len() && self.river_mask[idx] {
                    lines.push("RIVER".to_string());
                }
                // Traffic
                let traffic = self.traffic.get(ux, uy);
                if traffic > 1.0 {
                    lines.push(format!("traffic: {:.0}", traffic));
                }
                // Danger scent
                let danger = self.danger_scent.get(ux, uy);
                if danger > 0.01 {
                    lines.push(format!("danger: {:.2}", danger));
                }
            } else {
                lines.push(format!("({},{}) out of bounds", wx, wy));
            }
        }

        // Entity info — find all entities at this world position
        for (e, (pos, sprite)) in self
            .world
            .query::<(hecs::Entity, (&Position, &Sprite))>()
            .iter()
        {
            let ex = pos.x.round() as i32;
            let ey = pos.y.round() as i32;
            if ex == wx && ey == wy {
                lines.push("---".to_string());
                lines.push(format!("'{}' at ({:.1},{:.1})", sprite.ch, pos.x, pos.y));

                if let Ok(creature) = self.world.get::<&Creature>(e) {
                    let species_str = match creature.species {
                        Species::Prey => "Prey",
                        Species::Predator => "Predator",
                        Species::Villager => "Villager",
                    };
                    lines.push(species_str.to_string());
                    lines.push(format!("hunger: {:.1}%", creature.hunger * 100.0));
                    lines.push(format!("sight: {:.0}", creature.sight_range));
                    lines.push(format!(
                        "home: ({:.0},{:.0})",
                        creature.home_x, creature.home_y
                    ));
                }
                if let Ok(behavior) = self.world.get::<&Behavior>(e) {
                    let state_str = match &behavior.state {
                        BehaviorState::Wander { timer } => format!("Wander ({})", timer),
                        BehaviorState::Seek {
                            target_x,
                            target_y,
                            reason,
                        } => format!("Seek {:?} ({:.0},{:.0})", reason, target_x, target_y),
                        BehaviorState::Idle { timer } => format!("Idle ({})", timer),
                        BehaviorState::Eating { timer } => format!("Eating ({})", timer),
                        BehaviorState::FleeHome { timer } => format!("Fleeing home! ({})", timer),
                        BehaviorState::AtHome { timer } => format!("At home ({})", timer),
                        BehaviorState::Hunting { target_x, target_y } => {
                            format!("Hunting ({:.0},{:.0})", target_x, target_y)
                        }
                        BehaviorState::Captured => "CAPTURED!".to_string(),
                        BehaviorState::Gathering {
                            timer,
                            resource_type,
                        } => format!("Gathering {:?} ({})", resource_type, timer),
                        BehaviorState::Hauling {
                            target_x,
                            target_y,
                            resource_type,
                        } => format!(
                            "Hauling {:?} ({:.0},{:.0})",
                            resource_type, target_x, target_y
                        ),
                        BehaviorState::Sleeping { timer } => format!("Sleeping ({})", timer),
                        BehaviorState::Building {
                            target_x,
                            target_y,
                            timer,
                        } => format!("Building ({:.0},{:.0}) ({})", target_x, target_y, timer),
                        BehaviorState::Farming {
                            target_x, target_y, ..
                        } => {
                            format!("Farming ({:.0},{:.0})", target_x, target_y)
                        }
                        BehaviorState::Working {
                            target_x, target_y, ..
                        } => {
                            format!("Working ({:.0},{:.0})", target_x, target_y)
                        }
                        BehaviorState::Exploring {
                            target_x,
                            target_y,
                            timer,
                        } => {
                            format!("Exploring ({:.0},{:.0}) ({})", target_x, target_y, timer)
                        }
                    };
                    lines.push(format!("state: {}", state_str));
                    lines.push(format!("speed: {:.2}", behavior.speed));
                    match &behavior.state {
                        BehaviorState::Gathering { resource_type, .. }
                        | BehaviorState::Hauling { resource_type, .. } => {
                            lines.push(format!("resource: {:?}", resource_type));
                        }
                        _ => {}
                    }
                }
                if self.world.get::<&FoodSource>(e).is_ok() {
                    lines.push("Food Source".to_string());
                }
                if self.world.get::<&Den>(e).is_ok() {
                    lines.push("Den (safe zone)".to_string());
                }
                if self.world.get::<&StoneDeposit>(e).is_ok() {
                    lines.push("Stone Deposit".to_string());
                }
                if let Ok(site) = self.world.get::<&ecs::BuildSite>(e) {
                    lines.push(format!("BuildSite: {}", site.building_type.name()));
                    lines.push(format!("progress: {}/{}", site.progress, site.required));
                    lines.push(format!("assigned: {}", site.assigned));
                }
                if let Ok(farm) = self.world.get::<&FarmPlot>(e) {
                    let fert = self.soil_fertility.get(farm.tile_x, farm.tile_y);
                    if farm.fallow {
                        lines.push(format!("Farm: FALLOW (fertility {:.0}%)", fert * 100.0));
                    } else {
                        lines.push(format!(
                            "Farm: {:.0}% grown{}",
                            farm.growth * 100.0,
                            if farm.harvest_ready { " [READY]" } else { "" }
                        ));
                        lines.push(format!("  fertility: {:.0}%", fert * 100.0));
                    }
                }
                if self.world.get::<&Stockpile>(e).is_ok() {
                    lines.push(format!(
                        "Stockpile (F:{} W:{} S:{})",
                        self.resources.food, self.resources.wood, self.resources.stone
                    ));
                    lines.push(format!(
                        "  Planks:{} Masonry:{} Grain:{}",
                        self.resources.planks, self.resources.masonry, self.resources.grain
                    ));
                }
                if let Ok(pb) = self.world.get::<&ProcessingBuilding>(e) {
                    let recipe_str = match pb.recipe {
                        ecs::Recipe::WoodToPlanks => "2 Wood -> 1 Planks",
                        ecs::Recipe::StoneToMasonry => "2 Stone -> 1 Masonry",
                        ecs::Recipe::FoodToGrain => "3 Food -> 2 Grain",
                        ecs::Recipe::GrainToBread => "2 Grain+1 Wood -> 3 Bread",
                    };
                    let has_input = match pb.recipe {
                        ecs::Recipe::WoodToPlanks => self.resources.wood >= 2,
                        ecs::Recipe::StoneToMasonry => self.resources.stone >= 2,
                        ecs::Recipe::FoodToGrain => self.resources.food >= 3,
                        ecs::Recipe::GrainToBread => {
                            self.resources.grain >= 2 && self.resources.wood >= 1
                        }
                    };
                    let status = if has_input { "ACTIVE" } else { "IDLE" };
                    lines.push(format!("Recipe: {}", recipe_str));
                    lines.push(format!(
                        "Progress: {}/{} [{}]",
                        pb.progress, pb.required, status
                    ));
                }
            }
        }

        // Draw panel in top-right corner
        let panel_w = lines.iter().map(|l| l.len()).max().unwrap_or(0) + 2;
        let panel_h = lines.len();
        let panel_x = w.saturating_sub(panel_w as u16 + 1);
        let panel_y = 1u16;

        let bg = Color(20, 20, 40);
        let fg = Color(220, 220, 220);

        // Draw background
        for dy in 0..panel_h {
            let sy = panel_y + dy as u16;
            if sy >= h.saturating_sub(status_h) {
                break;
            }
            for dx in 0..panel_w {
                let sx = panel_x + dx as u16;
                if sx < w {
                    renderer.draw(sx, sy, ' ', fg, Some(bg));
                }
            }
        }

        // Draw text
        for (dy, line) in lines.iter().enumerate() {
            let sy = panel_y + dy as u16;
            if sy >= h.saturating_sub(status_h) {
                break;
            }
            for (dx, ch) in line.chars().enumerate() {
                let sx = panel_x + 1 + dx as u16;
                if sx < w {
                    renderer.draw(sx, sy, ch, fg, Some(bg));
                }
            }
        }
    }
}
