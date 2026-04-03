use rand::RngExt;

use super::{CELL_ASPECT, PANEL_WIDTH, ROAD_TRAFFIC_THRESHOLD};
#[allow(unused_imports)] // some used only in demolish_at
use crate::ecs::{
    self, BuildSite, BuildingType, Creature, FarmPlot, GarrisonBuilding, HutBuilding, Position,
    ProcessingBuilding, Recipe, Species, Stockpile, TownHallBuilding,
};
use crate::renderer::Renderer;
use crate::tilemap::Terrain;

impl super::Game {
    pub fn can_place_building(&self, bx: i32, by: i32, building_type: BuildingType) -> bool {
        self.can_place_building_impl(bx, by, building_type, true)
    }

    fn can_place_building_impl(
        &self,
        bx: i32,
        by: i32,
        building_type: BuildingType,
        require_influence: bool,
    ) -> bool {
        let (w, h) = building_type.size();
        for dy in 0..h {
            for dx in 0..w {
                let tx = bx + dx;
                let ty = by + dy;
                if tx < 0
                    || ty < 0
                    || tx as usize >= self.map.width
                    || ty as usize >= self.map.height
                {
                    return false;
                }
                if let Some(terrain) = self.map.get(tx as usize, ty as usize) {
                    match building_type {
                        BuildingType::Bridge => {
                            // Bridges can only be placed on Water tiles
                            if *terrain != Terrain::Water {
                                return false;
                            }
                        }
                        _ => match terrain {
                            Terrain::Grass | Terrain::Sand | Terrain::Forest => {} // ok
                            _ => return false, // water, mountain, snow, existing buildings
                        },
                    }
                } else {
                    return false;
                }
            }
        }
        // Check for overlapping build sites (finished buildings already rejected
        // by terrain check above — they set tiles to BuildingFloor/BuildingWall)
        for (pos, site) in self.world.query::<(&Position, &BuildSite)>().iter() {
            let (sw, sh) = site.building_type.size();
            let sx = pos.x as i32;
            let sy = pos.y as i32;
            if bx < sx + sw && bx + w > sx && by < sy + sh && by + h > sy {
                return false;
            }
        }

        // For player-initiated placement, must be within settlement influence.
        // Auto-build skips this check — it expands the settlement boundary.
        if require_influence {
            let in_territory = (0..h).any(|dy| {
                (0..w).any(|dx| {
                    let tx = bx + dx;
                    let ty = by + dy;
                    if tx >= 0 && ty >= 0 {
                        self.influence.get(tx as usize, ty as usize) > 0.1
                    } else {
                        false
                    }
                })
            });
            if !in_territory {
                return false;
            }
        }
        true
    }

    /// Try to place a building at the build cursor position.
    pub(super) fn try_place_building(&mut self) {
        let bx = self.build_cursor_x;
        let by = self.build_cursor_y;
        let bt = self.selected_building;

        if !self.can_place_building(bx, by, bt) {
            return;
        }

        // Check resources
        let cost = bt.cost();
        if !self.resources.can_afford(&cost) {
            return;
        }

        // Deduct resources
        self.resources.deduct(&cost);

        self.place_build_site(bx, by, bt);
    }

    /// Place a build site: reserve footprint tiles and spawn the entity.
    pub(super) fn place_build_site(&mut self, bx: i32, by: i32, bt: BuildingType) {
        let (sw, sh) = bt.size();

        // Clear terrain features (stones, berry bushes, dens) within footprint
        let mut to_remove = Vec::new();
        for (entity, pos) in self.world.query::<(hecs::Entity, &Position)>().iter() {
            let px = pos.x.round() as i32;
            let py = pos.y.round() as i32;
            if px >= bx && px < bx + sw && py >= by && py < by + sh {
                // Only remove terrain features, not villagers/creatures with Behavior
                if self.world.get::<&ecs::FoodSource>(entity).is_ok()
                    || self.world.get::<&ecs::StoneDeposit>(entity).is_ok()
                    || self.world.get::<&ecs::Den>(entity).is_ok()
                {
                    to_remove.push(entity);
                }
            }
        }
        for entity in to_remove {
            let _ = self.world.despawn(entity);
        }

        // Bridge uses its own terrain type; all others use BuildingFloor as placeholder
        let placeholder_terrain = if bt == BuildingType::Bridge {
            Terrain::Bridge
        } else {
            Terrain::BuildingFloor
        };
        for dy in 0..sh {
            for dx in 0..sw {
                let tx = bx + dx;
                let ty = by + dy;
                if tx >= 0 && ty >= 0 {
                    self.map.set(tx as usize, ty as usize, placeholder_terrain);
                }
            }
        }
        ecs::spawn_build_site(&mut self.world, bx as f64, by as f64, bt, self.tick);
    }

    /// Handle a mouse click at screen coordinates.
    pub(super) fn handle_mouse_click(&mut self, sx: u16, sy: u16, renderer: &dyn Renderer) {
        let (_w, h) = renderer.size();

        // Click in panel area — handle panel buttons
        if sx < PANEL_WIDTH {
            self.handle_panel_click(sy, h);
            return;
        }

        // Click in map area — convert screen coords to world coords
        let map_sx = sx - PANEL_WIDTH;
        let wx = self.camera.x + map_sx as i32 / CELL_ASPECT;
        let wy = self.camera.y + sy as i32;

        if self.build_mode {
            // Move build cursor and place
            self.build_cursor_x = wx;
            self.build_cursor_y = wy;
            self.try_place_building();
        } else {
            // Enter query mode at clicked position
            self.query_mode = true;
            self.query_cx = wx;
            self.query_cy = wy;
        }
    }

    /// Handle clicks on the left panel buttons.
    pub(super) fn handle_panel_click(&mut self, sy: u16, _h: u16) {
        // Panel layout (row positions):
        // 0: header
        // 1: blank
        // 2-3: date/season
        // 4: blank
        // 5-7: resources
        // 8: blank
        // 9-11: population
        // 12: blank
        // 13: "-- Build --"
        // 14+: building type buttons
        // After buildings: auto-build toggle
        let building_start = 14u16;
        let types = BuildingType::all();
        let auto_build_row = building_start + types.len() as u16 + 1;

        if sy >= building_start && sy < building_start + types.len() as u16 {
            let idx = (sy - building_start) as usize;
            if idx < types.len() {
                self.selected_building = types[idx];
                self.build_mode = true;
                self.query_mode = false;
            }
        } else if sy == auto_build_row {
            self.auto_build = !self.auto_build;
        }
    }

    /// Compute the average position of all villagers as the settlement center.
    pub fn settlement_center(&self) -> (i32, i32) {
        let positions: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if positions.is_empty() {
            return (128, 128);
        }
        let cx = positions.iter().map(|p| p.0).sum::<f64>() / positions.len() as f64;
        let cy = positions.iter().map(|p| p.1).sum::<f64>() / positions.len() as f64;
        (cx as i32, cy as i32)
    }

    /// Compute a defense rating from garrison buildings, wall tiles, and military skill.
    pub(super) fn compute_defense_rating(&self) -> f64 {
        let garrison_defense: f64 = self
            .world
            .query::<&GarrisonBuilding>()
            .iter()
            .map(|g| g.defense_bonus)
            .sum();

        let (cx, cy) = self.settlement_center();
        let mut wall_tiles = 0u32;
        for dy in -20i32..=20 {
            for dx in -20i32..=20 {
                let tx = cx + dx;
                let ty = cy + dy;
                if tx >= 0
                    && ty >= 0
                    && let Some(Terrain::BuildingWall) = self.map.get(tx as usize, ty as usize)
                {
                    wall_tiles += 1;
                }
            }
        }

        garrison_defense + wall_tiles as f64 * 0.5 + self.skills.military * 0.2
    }

    /// Check for completed build sites and apply their tiles to the map.
    pub(super) fn check_build_completion(&mut self) {
        let mut completed: Vec<(hecs::Entity, Position, BuildSite)> = Vec::new();
        for (e, (pos, site)) in self
            .world
            .query::<(hecs::Entity, (&Position, &BuildSite))>()
            .iter()
        {
            if site.progress >= site.required {
                completed.push((e, *pos, *site));
            }
        }
        for &(e, pos, site) in &completed {
            for (dx, dy, terrain) in site.building_type.tiles() {
                let tx = pos.x as i32 + dx;
                let ty = pos.y as i32 + dy;
                if tx >= 0 && ty >= 0 {
                    self.map.set(tx as usize, ty as usize, terrain);
                }
            }
            // Spawn building entities for completed buildings
            if site.building_type == BuildingType::Hut {
                let (sw, sh) = site.building_type.size();
                let cx = pos.x + sw as f64 / 2.0;
                let cy = pos.y + sh as f64 / 2.0;
                ecs::spawn_hut(&mut self.world, cx, cy);
            }
            if site.building_type == BuildingType::Farm {
                let (sw, sh) = site.building_type.size();
                let cx = pos.x + sw as f64 / 2.0;
                let cy = pos.y + sh as f64 / 2.0;
                ecs::spawn_farm_plot(&mut self.world, cx, cy);
            }
            if site.building_type == BuildingType::Workshop {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::WoodToPlanks);
            }
            if site.building_type == BuildingType::Smithy {
                ecs::spawn_processing_building(
                    &mut self.world,
                    pos.x,
                    pos.y,
                    Recipe::StoneToMasonry,
                );
            }
            if site.building_type == BuildingType::Garrison {
                ecs::spawn_garrison(&mut self.world, pos.x, pos.y);
            }
            if site.building_type == BuildingType::TownHall {
                ecs::spawn_town_hall(&mut self.world, pos.x, pos.y);
            }
            if site.building_type == BuildingType::Granary {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::FoodToGrain);
            }
            if site.building_type == BuildingType::Bakery {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::GrainToBread);
            }
            self.world.despawn(e).ok();
        }
        for &(_, _, site) in &completed {
            self.notify(format!("Building complete: {}", site.building_type.name()));
        }
    }

    /// Collect influence sources from villagers and active build sites, then update.
    pub fn update_influence(&mut self) {
        let mut sources: Vec<(f64, f64, f64)> = Vec::new();

        // Villagers emit influence at strength 1.0
        for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
            if creature.species == Species::Villager {
                sources.push((pos.x, pos.y, 1.0));
            }
        }

        // Active build sites emit influence at strength 0.5
        for (pos, _site) in self.world.query::<(&Position, &BuildSite)>().iter() {
            sources.push((pos.x, pos.y, 0.5));
        }

        // Garrisons project stronger influence (outpost expansion)
        for (pos, _) in self.world.query::<(&Position, &GarrisonBuilding)>().iter() {
            sources.push((pos.x, pos.y, 3.0));
        }

        // Town Hall projects the widest influence — the civic heart of the settlement
        for (pos, _) in self.world.query::<(&Position, &TownHallBuilding)>().iter() {
            sources.push((pos.x, pos.y, 5.0));
        }

        // Huts and stockpiles emit moderate influence
        for (pos, _) in self.world.query::<(&Position, &HutBuilding)>().iter() {
            sources.push((pos.x, pos.y, 1.5));
        }
        for (pos, _) in self.world.query::<(&Position, &Stockpile)>().iter() {
            sources.push((pos.x, pos.y, 1.0));
        }

        self.influence.update(&sources, None);
    }

    /// Track villager movement and auto-convert high-traffic tiles to roads.
    pub fn update_traffic(&mut self) {
        // Record footsteps for all villagers
        for (pos, creature) in self.world.query::<(&Position, &Creature)>().iter() {
            if creature.species == Species::Villager {
                let ix = pos.x.round() as usize;
                let iy = pos.y.round() as usize;
                self.traffic.step_on(ix, iy);
            }
        }

        // Slow decay every 10 ticks
        if self.tick.is_multiple_of(10) {
            self.traffic.decay();
        }

        // Check for road conversion every 100 ticks
        if self.tick.is_multiple_of(100) {
            let candidates = self
                .traffic
                .road_candidates(&self.map, ROAD_TRAFFIC_THRESHOLD);
            for (x, y) in candidates {
                self.map.set(x, y, Terrain::Road);
            }
        }
    }

    /// Spawn new stone deposits near the settlement center when stone stockpile is critically low.
    /// DEPRECATED: Resources should be discovered through exploration, not spawned.
    /// Kept as dead code for reference. See docs/design/pillar1_geography/precomputed_resource_map.md
    #[allow(dead_code)]
    fn discover_stone_deposits(&mut self) {
        let villager_pos: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if villager_pos.is_empty() {
            return;
        }
        let cx = villager_pos.iter().map(|p| p.0).sum::<f64>() / villager_pos.len() as f64;
        let cy = villager_pos.iter().map(|p| p.1).sum::<f64>() / villager_pos.len() as f64;

        let mut rng = rand::rng();
        let mut spawned = 0u32;
        // Two passes: first prefer Grass/Sand/Forest (fast mining), then any walkable tile.
        // On mountain-heavy seeds, deposits on mountain terrain (0.25× speed) mine far too
        // slowly to accumulate enough stone for buildings. Grass/Sand deposits mine at full
        // speed, so settlements receive usable stone much sooner.
        for pass in 0..2u32 {
            if spawned >= 2 {
                break;
            }
            for _ in 0..60 {
                if spawned >= 2 {
                    break;
                }
                let angle = rng.random_range(0.0f64..std::f64::consts::TAU);
                let d = rng.random_range(5.0f64..18.0); // within villager sight_range (22 tiles)
                let tx = cx + angle.cos() * d;
                let ty = cy + angle.sin() * d;
                if tx < 0.0 || ty < 0.0 || !self.map.is_walkable(tx, ty) {
                    continue;
                }
                let on_easy_terrain = matches!(
                    self.map.get(tx as usize, ty as usize),
                    Some(Terrain::Grass | Terrain::Sand | Terrain::Forest)
                );
                if pass == 0 && !on_easy_terrain {
                    continue; // first pass: easy terrain only
                }
                ecs::spawn_stone_deposit(&mut self.world, tx, ty);
                spawned += 1;
            }
        }
        if spawned > 0 {
            self.notify(format!(
                "New stone deposit discovered! (+{} deposits)",
                spawned
            ));
        }
    }

    /// DEPRECATED: Resources should be discovered through exploration, not spawned.
    /// Kept as dead code for reference. See docs/design/pillar1_geography/deforestation_regrowth.md
    #[allow(dead_code)]
    pub(super) fn discover_timber_grove(&mut self) {
        let villager_pos: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if villager_pos.is_empty() {
            return;
        }
        let cx = villager_pos.iter().map(|p| p.0).sum::<f64>() / villager_pos.len() as f64;
        let cy = villager_pos.iter().map(|p| p.1).sum::<f64>() / villager_pos.len() as f64;

        // Systematic ring scan rather than random angles — random approaches fail on maps
        // where buildable land lies in a narrow sector (e.g. peninsulas, cliff edges).
        // Scan outward ring-by-ring at 1-tile steps; first valid anchor wins.
        let cxi = cx as i32;
        let cyi = cy as i32;
        let mut grove_planted = false;
        'outer: for r in 5i32..22 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue; // perimeter only
                    }
                    let tx = cxi + dx;
                    let ty = cyi + dy;
                    if tx < 0 || ty < 0 {
                        continue;
                    }
                    let tx = tx as usize;
                    let ty = ty as usize;
                    let anchor_terrain = self.map.get(tx, ty);
                    // Skip out-of-bounds and built structures; accept all walkable terrain
                    // including Forest (previously excluded, causing silent infinite failure).
                    let is_built = matches!(
                        anchor_terrain,
                        Some(Terrain::BuildingFloor | Terrain::BuildingWall | Terrain::Road)
                    );
                    if is_built || anchor_terrain.is_none() {
                        continue;
                    }
                    if !self.map.is_walkable(tx as f64, ty as f64)
                        && !matches!(anchor_terrain, Some(Terrain::Mountain))
                    {
                        continue; // skip Water and other non-walkable non-Mountain
                    }

                    // Inner 3×3 core → Grass (clean buildable footprint for future hut/workshop).
                    // Outer ring → Forest (wood resource for gatherers).
                    let mut count = 0u32;
                    for ody in -2i32..=2 {
                        for odx in -2i32..=2 {
                            let fx = tx as i32 + odx;
                            let fy = ty as i32 + ody;
                            if fx < 0 || fy < 0 {
                                continue;
                            }
                            let fx = fx as usize;
                            let fy = fy as usize;
                            if matches!(
                                self.map.get(fx, fy),
                                Some(
                                    Terrain::BuildingFloor | Terrain::BuildingWall | Terrain::Road
                                )
                            ) {
                                continue;
                            }
                            let is_core = odx.abs() <= 1 && ody.abs() <= 1;
                            if is_core {
                                if !matches!(self.map.get(fx, fy), Some(Terrain::Grass)) {
                                    self.map.set(fx, fy, Terrain::Grass);
                                    count += 1;
                                }
                            } else if !matches!(self.map.get(fx, fy), Some(Terrain::Forest)) {
                                self.map.set(fx, fy, Terrain::Forest);
                                count += 1;
                            }
                        }
                    }
                    if count >= 3 {
                        self.notify(format!("Timber grove discovered! ({count} new tiles)"));
                        grove_planted = true;
                        break 'outer;
                    }
                }
            }
        }
        let _ = grove_planted; // suppress unused warning
    }

    /// Check conditions and spawn a new villager if met.
    /// Births require: 2+ villagers, food >= 5, and housing capacity.
    /// More surplus housing = shorter birth cooldown (min 200, max 800 ticks).
    pub(super) fn try_population_growth(&mut self) {
        let villager_count = self
            .world
            .query::<&Creature>()
            .iter()
            .filter(|c| c.species == Species::Villager)
            .count() as u32;

        // Count total housing capacity: huts + any Town Hall bonus
        let hut_capacity: u32 = self
            .world
            .query::<&HutBuilding>()
            .iter()
            .map(|h| h.capacity)
            .sum();
        let town_hall_bonus: u32 = self
            .world
            .query::<&TownHallBuilding>()
            .iter()
            .map(|t| t.housing_bonus)
            .sum();
        let total_capacity = hut_capacity + town_hall_bonus;

        // Housing surplus determines birth rate
        let housing_surplus = total_capacity.saturating_sub(villager_count);
        let base_cooldown = if housing_surplus == 0 {
            return; // No births without housing surplus
        } else if housing_surplus >= 4 {
            200 // Fast growth when lots of empty housing
        } else {
            800 / housing_surplus as u64 // 800, 400, 266 for surplus 1, 2, 3
        };

        // Seasonal birth rate: spring 1.2x (shorter cooldown), winter 0.5x (longer cooldown)
        let birth_mult = self.day_night.season_modifiers().birth_rate_mult;
        let birth_cooldown = (base_cooldown as f64 / birth_mult) as u64;

        if self.tick - self.last_birth_tick <= birth_cooldown {
            return;
        }

        // Count grain as food equivalent (1 grain = 0.5 food, since it takes ~2 food to make 1
        // grain via granary). Bread counts as food directly. This prevents the deadlock where
        // food=0 but grain=400+ blocks births — grain is food, just stored in a different form.
        let effective_food = self.resources.food + self.resources.grain / 2 + self.resources.bread;

        // Require minimum food proportional to population to prevent growing into starvation.
        // 2× pop threshold (vs just food >= 5) prevents births during food crises on large
        // populations, while remaining loose enough not to choke small settlements.
        let min_food = if villager_count > 10 {
            (villager_count * 2).max(5)
        } else {
            5
        };
        if villager_count < 2 || effective_food < min_food {
            return;
        }

        // Food security gate: prevent breeding into starvation at larger populations
        if villager_count > 10 && effective_food < villager_count * 3 {
            return;
        }

        // Consume 5 food equivalent (use grain if food is short — grain counts 2:1)
        if self.resources.food >= 5 {
            self.resources.food -= 5;
        } else {
            let from_grain = (5 - self.resources.food) * 2;
            self.resources.food = 0;
            self.resources.grain = self.resources.grain.saturating_sub(from_grain);
        }

        // Collect villager positions to find a spawn point nearby
        let villager_pos: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();

        if let Some(&(vx, vy)) = villager_pos.first()
            && let Some((nx, ny)) = self.find_nearby_walkable(vx, vy, 5)
        {
            ecs::spawn_villager_staggered(&mut self.world, nx, ny, self.tick);
            self.last_birth_tick = self.tick;
            self.notify("New villager born!".to_string());
        }
    }

    /// Find a walkable tile within `radius` of (cx, cy).
    pub(super) fn find_nearby_walkable(&self, cx: f64, cy: f64, radius: i32) -> Option<(f64, f64)> {
        for r in 0..=radius {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue; // only check perimeter of each ring
                    }
                    let nx = cx + dx as f64;
                    let ny = cy + dy as f64;
                    if self.map.is_walkable(nx, ny) {
                        return Some((nx, ny));
                    }
                }
            }
        }
        None
    }

    /// Auto-build: place buildings automatically based on settlement needs.
    pub(super) fn auto_build_tick(&mut self) {
        // Find settlement center from villager positions
        let villager_pos: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if villager_pos.is_empty() {
            return;
        }
        let cx = villager_pos.iter().map(|p| p.0).sum::<f64>() / villager_pos.len() as f64;
        let cy = villager_pos.iter().map(|p| p.1).sum::<f64>() / villager_pos.len() as f64;

        // Stone deposit discovery: every 2000 ticks, spawn 2 new deposits if stockpile
        // stone is low or all existing deposits are depleted. This prevents stone starvation
        // when the initial 2 deposits run out (~10 stone each).
        if self.tick % 2000 == 0 {
            let stone_deposit_count = self.world.query::<(&ecs::StoneDeposit,)>().iter().count();
            if stone_deposit_count == 0 || self.resources.stone < 20 {
                // Place new deposits at alternating angles around settlement center.
                // Keep within villager sight_range (22 tiles) so they can be found and mined.
                let cycle = (self.tick / 2000) as f64;
                let base_angle = cycle * std::f64::consts::PI * 0.618; // golden-ratio rotation
                let dist = 8.0 + (cycle % 4.0) * 3.0; // 8, 11, 14, 17 tiles — always in sight range
                for i in 0..2 {
                    let angle = base_angle + (i as f64) * std::f64::consts::PI;
                    let tx = cx + angle.cos() * dist;
                    let ty = cy + angle.sin() * dist;
                    if let Some((nx, ny)) = self.find_nearby_walkable(tx, ty, 6) {
                        ecs::spawn_stone_deposit(&mut self.world, nx, ny);
                    }
                }
                self.notify("Stone deposit discovered nearby!".to_string());
            }
        }

        // Count existing farms (completed + in-progress)
        let farm_count = self.world.query::<&FarmPlot>().iter().count()
            + self
                .world
                .query::<&BuildSite>()
                .iter()
                .filter(|s| s.building_type == BuildingType::Farm)
                .count();

        // Priority 1 & 2: Farm and Hut are both unconditional — they run together before any
        // optional-build cap. Both may queue in the same tick (farm deducts first; hut checks
        // can_afford on what remains). This prevents the scenario where food demand always fires
        // P1 and returns before P2, starving housing construction and blocking population growth.
        let villager_count = villager_pos.len() as u32;
        let mut queued_critical = false;

        // Hut capacity — pre-computed here (before P1 Farm) so P1 can check whether housing is
        // already full. When housing is at capacity no new villagers can be born, so building
        // extra farms is wasteful: it consumes 5w that P2 Hut needs (6w) and keeps wood stuck
        // at ≤1 indefinitely. Defer farms when housing is saturated and at least 2 farms exist.
        let completed_huts = self.world.query::<&HutBuilding>().iter().count();
        // Only count pending huts that are "active": either have made progress or were placed
        // recently (< 500 ticks ago). Stale zero-progress huts (stuck, unreachable) are excluded
        // so they don't inflate total_hut_capacity and block new hut queuing.
        let pending_huts = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Hut)
            .filter(|s| s.progress > 0 || self.tick.saturating_sub(s.queued_at) < 500)
            .count();
        let total_hut_capacity = (completed_huts + pending_huts) * 4;

        // Pre-compute has_granary / pending_granary_any for use in P4 below.
        let has_granary = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::FoodToGrain);
        let pending_granary_any = self
            .world
            .query::<&BuildSite>()
            .iter()
            .any(|s| s.building_type == BuildingType::Granary);

        // Pre-compute has_workshop / pending_workshop_any — used in P0.5, P2 fallback, and P3.
        let has_workshop = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::WoodToPlanks);
        let pending_workshop_any = self
            .world
            .query::<&BuildSite>()
            .iter()
            .any(|s| s.building_type == BuildingType::Workshop);

        // Pre-compute has_smithy / pending_smithy — used in saving_for_smithy guard (P2) and P5.
        let has_smithy = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::StoneToMasonry);
        let pending_smithy = self
            .world
            .query::<&BuildSite>()
            .iter()
            .any(|s| s.building_type == BuildingType::Smithy);

        // Priority 0.5: Workshop — pre-empts Farm when food security is proven.
        // Without this, P1 Farm always takes wood=5 first (Farm costs 5w, Workshop costs 5w),
        // permanently preventing Workshop from ever being built.
        // Two signals for food security:
        //   (a) grain >= pop*4: Granary is running and accumulating a buffer, OR
        //   (b) food > 60 + pop*6: raw food surplus is large enough that even if Granary
        //       workers compete with farm workers, the settlement clearly has enough food.
        // (a) is preferred but (b) prevents the deadlock when many farms fill all worker
        // slots leaving no capacity for the Granary.
        // food_secure: either grain buffer or surplus food, OR early-game (pop≤5) with any food
        // present — at pop=4, food=20 satisfies this so Workshop fires immediately when affordable.
        let food_secure = self.resources.grain >= villager_count * 2
            || self.resources.food > villager_count * 4 + 20
            || (villager_count <= 5 && self.resources.food >= 10);
        if !has_workshop
            && !pending_workshop_any
            && villager_count >= 4
            && self.resources.stone >= 3
            && food_secure
        {
            let cost = BuildingType::Workshop.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Workshop)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Workshop);
                self.notify("Auto-build: Workshop queued".to_string());
                return; // Don't queue Farm or Hut this tick — Workshop is the priority
            }
        }

        // Priority 0.9: Garrison — checked BEFORE P1 Farm so farm/hut stone spend doesn't
        // prevent garrison from ever being placed.  P1 deducts 1s and P2 deducts 3s per cycle;
        // if garrison fired at P1.5 it always saw a stone budget 1 below the garrison threshold.
        // Moving it here ensures the full stone budget is visible to the garrison check.
        // Cost is 6w+8s — achievable with 1-2 nearby deposits from the stone-range fix.
        let has_garrison = self.world.query::<&GarrisonBuilding>().iter().count() > 0;
        let pending_garrison = self
            .world
            .query::<&BuildSite>()
            .iter()
            .any(|s| s.building_type == BuildingType::Garrison);
        if !has_garrison && !pending_garrison && villager_count >= 3 && self.resources.stone >= 8 {
            let cost = BuildingType::Garrison.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Garrison)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Garrison);
                self.notify("Auto-build: Garrison queued".to_string());
                queued_critical = true;
            }
        }

        // Priority 1: Farm when food is low and we don't have enough farms.
        // Threshold: 1 farm per 3 villagers (div_ceil), minimum 2. The old "2/3 of pop"
        // threshold (e.g. 6 farms for pop=8) was far too high — one farm already produces
        // ~25 food/1000 ticks while 8 villagers consume only ~1.2 food/1000 ticks. The old
        // threshold caused P1 to fire every 50 ticks until 6 farms existed, permanently
        // blocking P3 (Workshop) from ever running. Minimum of 2 ensures early-game (pop=3)
        // still queues a second farm before the threshold is satisfied.
        //
        // Housing-saturation guard: when total_hut_capacity <= villager_count the settlement
        // is housing-capped and no new births can happen regardless of food. Building extra
        // farms in this state consumes 5w per farm and prevents P2 Hut (6w) from ever firing,
        // creating a permanent wood=1 equilibrium. Allow farms only when ≥1 housing slot is
        // available OR when fewer than 2 farms exist (minimum food-security floor).
        let housing_at_cap = total_hut_capacity <= villager_count as usize;
        if self.resources.food < 8 + villager_count * 4
            && farm_count < (villager_count as usize).div_ceil(3) + 1
            && (!housing_at_cap || farm_count < 2)
        {
            let cost = BuildingType::Farm.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Farm)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Farm);
                self.notify("Auto-build: Farm queued".to_string());
                queued_critical = true;
            }
        }

        // Priority 2: Hut when population needs housing
        // Runs in the same tick as P1 so farm demand never permanently blocks housing.
        // (completed_huts, pending_huts, total_hut_capacity pre-computed above P1)
        // Defer hut construction only before the Workshop is built/queued: wood is depleted
        // by hut builds before it can accumulate to Workshop cost. Once a Workshop exists,
        // let hut builds proceed freely — the old (has_workshop && planks==0) guard was
        // creating a deadlock where wood stayed at 0 (consumed by Workshop cycling), planks
        // stayed at 0, and huts were permanently blocked, capping population at 16.
        let saving_for_workshop = !has_workshop
            && !pending_workshop_any
            && villager_count >= 4
            && self.resources.stone >= 3
            && self.resources.grain >= villager_count * 4;
        // Saving-for-smithy guard: when Workshop exists, stone > 10, and Smithy not yet built,
        // defer huts until wood ≥ 14.  WoodToPlanks threshold is 12: Workshop processes at
        // wood=12 → leaves wood=10 → auto_build_tick sees wood=10 and can afford Smithy (10w+15s)
        // before any hut (6w) takes it.  Housing crisis (capacity ≤ count) overrides deferral.
        let at_housing_crisis = total_hut_capacity <= villager_count as usize;
        let saving_for_smithy =
            has_workshop && !has_smithy && !pending_smithy && self.resources.stone > 10;
        let hut_ok = (!saving_for_workshop || self.resources.wood >= 10)
            && (!saving_for_smithy || at_housing_crisis || self.resources.wood >= 14);
        if hut_ok && total_hut_capacity < villager_count as usize + 4 && villager_count >= 3 {
            let cost = BuildingType::Hut.cost();
            let spot = self.find_building_spot(cx, cy, BuildingType::Hut);
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = spot
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Hut);
                self.notify("Auto-build: Hut queued".to_string());
                queued_critical = true;
            } else if !has_workshop
                && !pending_workshop_any
                && villager_count >= 4
                && self.resources.stone >= 3
            {
                // Housing is needed but we can't afford a hut right now.
                // Use the wood for Workshop instead — planks unlock Garrison which is
                // critical for wolf defense and stops the entire settlement from being wiped.
                let workshop_cost = BuildingType::Workshop.cost();
                if self.resources.can_afford(&workshop_cost)
                    && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Workshop)
                {
                    self.resources.deduct(&workshop_cost);
                    self.place_build_site(bx, by, BuildingType::Workshop);
                    self.notify("Auto-build: Workshop queued".to_string());
                    queued_critical = true;
                }
                // else: no valid terrain — villagers will explore and discover buildable areas
            }
            // else: housing needed but no terrain — exploration will reveal buildable land
        }

        if queued_critical {
            return;
        }

        // Count existing build sites being worked on
        let pending_builds = self.world.query::<&BuildSite>().iter().count();
        // Don't queue too many optional/processing builds at once
        if pending_builds >= 3 {
            return;
        }

        // Priority 3: First Workshop — also queued here when housing is satisfied
        // (has_workshop and pending_workshop_any pre-computed before P2 above).
        if !has_workshop
            && !pending_workshop_any
            && villager_count >= 4
            && self.resources.stone >= 3
        {
            let cost = BuildingType::Workshop.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Workshop)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Workshop);
                self.notify("Auto-build: Workshop queued".to_string());
                return;
            }
        }

        // Priority 4: First Granary when population is established and food is adequate.
        // (pending_granary_any and has_granary defined in P1.5 block above)
        if !has_granary && !pending_granary_any && villager_count >= 12 && self.resources.food > 80
        {
            let cost = BuildingType::Granary.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Granary)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Granary);
                self.notify("Auto-build: Granary queued".to_string());
                return;
            }
        }

        // Priority 3.5: Second Workshop when wood is accumulating faster than one can process
        let workshop_count = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .filter(|pb| pb.recipe == Recipe::WoodToPlanks)
            .count();
        let pending_workshop_count = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Workshop)
            .count();
        if has_workshop
            && workshop_count + pending_workshop_count < 2
            && self.resources.wood > 1000
            && self.resources.stone > 20
        {
            let cost = BuildingType::Workshop.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Workshop)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Workshop);
                self.notify("Auto-build: Workshop queued".to_string());
                return;
            }
        }

        // Priority 3.7: Third Workshop when wood is still piling up despite two workshops
        let pending_workshop_count_all = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Workshop)
            .count();
        if workshop_count >= 2
            && workshop_count + pending_workshop_count_all < 3
            && self.resources.wood > 4000
            && self.resources.stone > 10
        {
            let cost = BuildingType::Workshop.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Workshop)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Workshop);
                self.notify("Auto-build: Workshop queued".to_string());
                return;
            }
        }

        // Priority 5: Smithy when we have a Workshop and a stone surplus
        // (has_smithy and pending_smithy pre-computed above P1)
        // Threshold 10: stone discovery events give ~24 stone per cycle (2×12 yield); that
        // temporarily pushes stone above 10 even on desert maps. Old threshold of 25 was never
        // reached on most seeds (stone equilibrated at 7–9), blocking Masonry production.
        if !has_smithy && !pending_smithy && has_workshop && self.resources.stone > 10 {
            let cost = BuildingType::Smithy.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Smithy)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Smithy);
                self.notify("Auto-build: Smithy queued".to_string());
                return;
            }
        }

        // Priority 5.1: Second Smithy when stone is over-abundant (e.g. grassland maps mining mountains)
        // One Smithy can't absorb 3000+ stone; a second doubles masonry output and sinks stone.
        let smithy_count = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .filter(|pb| pb.recipe == Recipe::StoneToMasonry)
            .count();
        let pending_smithy_count = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Smithy)
            .count();
        if has_smithy && smithy_count + pending_smithy_count < 2 && self.resources.stone > 300 {
            let cost = BuildingType::Smithy.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Smithy)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Smithy);
                self.notify("Auto-build: Smithy queued".to_string());
                return;
            }
        }

        // Priority 5.2: Garrison fallback — in case P1.5 never triggered (stone stayed below 8
        // due to heavy farm/hut demand during the opening phase).  Same logic as P1.5 but fires
        // here as a safety net.  Garrison costs 6w+8s.
        let wolves_present = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .any(|(_, c)| c.species == Species::Predator);
        if !has_garrison && !pending_garrison && villager_count >= 3 && self.resources.stone >= 8 {
            let cost = BuildingType::Garrison.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Garrison)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Garrison);
                self.notify("Auto-build: Garrison queued".to_string());
                return;
            }
        }

        // Priority 5.3: Second Garrison when masonry is abundant and Year 2+ threats grow.
        // Two garrisons provide doubled defense and expand settlement influence.
        let garrison_count = self.world.query::<&GarrisonBuilding>().iter().count();
        let pending_garrison_count = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Garrison)
            .count();
        if has_garrison
            && garrison_count + pending_garrison_count < 2
            && self.resources.masonry >= 150
            && (wolves_present || villager_count >= 80)
        {
            let cost = BuildingType::Garrison.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Garrison)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Garrison);
                self.notify("Auto-build: Garrison queued".to_string());
                return;
            }
        }

        // Priority 5.45: Town Hall when masonry is abundant and settlement is well-established.
        // Town Hall (20w+30s+80m) is the largest masonry sink — provides 20 housing slots and
        // expands settlement influence (5.0 strength, highest of any building). Only 1 allowed.
        let has_town_hall = self.world.query::<&TownHallBuilding>().iter().count() > 0;
        let pending_town_hall = self
            .world
            .query::<&BuildSite>()
            .iter()
            .any(|s| s.building_type == BuildingType::TownHall);
        if !has_town_hall
            && !pending_town_hall
            && self.resources.masonry >= 80
            && self.resources.stone >= 30
            && villager_count >= 80
        {
            let cost = BuildingType::TownHall.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::TownHall)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::TownHall);
                self.notify("Auto-build: Town Hall queued".to_string());
                return;
            }
        }

        // Priority 5.5: Bakery when we have a Granary (grain available) and planks
        let has_bakery = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .any(|pb| pb.recipe == Recipe::GrainToBread);
        let pending_bakery = self
            .world
            .query::<&BuildSite>()
            .iter()
            .any(|s| s.building_type == BuildingType::Bakery);
        if !has_bakery
            && !pending_bakery
            && has_granary
            && self.resources.planks >= 8
            && self.resources.grain > 30
        {
            let cost = BuildingType::Bakery.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Bakery)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Bakery);
                self.notify("Auto-build: Bakery queued".to_string());
                return;
            }
        }

        // Priority 5.6: Second Granary when planks are available and grain supply is low.
        // With Bakery now using planks instead of wood, planks flow: Workshop->Bakery.
        // A second Granary (food->grain) ensures grain supply keeps up with two bakeries.
        let granary_count = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .filter(|pb| pb.recipe == Recipe::FoodToGrain)
            .count();
        let pending_granary_count = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Granary)
            .count();
        if has_granary
            && has_bakery
            && granary_count + pending_granary_count < 2
            && self.resources.planks > 100
            && self.resources.food > villager_count * 3
        {
            let cost = BuildingType::Granary.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Granary)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Granary);
                self.notify("Auto-build: Granary queued".to_string());
                return;
            }
        }

        // Priority 5.7: Second Bakery when planks are abundant and grain supply can support it.
        // Two bakeries double bread output, feeding larger populations, and drain planks faster.
        let bakery_count = self
            .world
            .query::<&ProcessingBuilding>()
            .iter()
            .filter(|pb| pb.recipe == Recipe::GrainToBread)
            .count();
        let pending_bakery_count = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Bakery)
            .count();
        if has_bakery
            && granary_count >= 2
            && bakery_count + pending_bakery_count < 2
            && self.resources.planks > 200
            && self.resources.grain > 80
        {
            let cost = BuildingType::Bakery.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Bakery)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Bakery);
                self.notify("Auto-build: Bakery queued".to_string());
                return;
            }
        }

        // Priority 6: Walls when wolves are nearby settlement
        let wolf_near = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Predator)
            .any(|(p, _)| {
                let dx = p.x - cx;
                let dy = p.y - cy;
                dx * dx + dy * dy < 400.0 // within ~20 tiles
            });
        if wolf_near {
            let cost = BuildingType::Wall.cost();
            if self.resources.can_afford(&cost) {
                // Place wall between settlement center and nearest wolf
                if let Some((bx, by)) = self.find_wall_spot(cx, cy) {
                    self.resources.deduct(&cost);
                    self.place_build_site(bx, by, BuildingType::Wall);
                    self.notify("Auto-build: Wall queued".to_string());
                }
            }
        }
    }

    /// BFS flood-fill to find all walkable tiles reachable from (cx, cy) within `radius` tiles.
    /// Used by `find_building_spot` to filter out terrain-valid but unreachable spots (e.g.
    /// land across a water body). Returns a flat bitset indexed by y*width+x.
    ///
    /// Pending build-site footprints are treated as SOLID during the BFS even though their tiles
    /// are currently BuildingFloor (walkable). This prevents placing new buildings behind a
    /// pending build site that will later wall off access when it completes.
    fn reachable_tiles(&self, cx: f64, cy: f64, radius: i32) -> Vec<bool> {
        let w = self.map.width;
        let h = self.map.height;
        let mut visited = vec![false; w * h];
        let si = cx.round() as i32;
        let sj = cy.round() as i32;
        if si < 0 || sj < 0 || si >= w as i32 || sj >= h as i32 {
            return visited;
        }

        // Mark all pending build-site footprint tiles as blocked (they will become walls).
        let mut pending_blocked = vec![false; w * h];
        for (pos, site) in self.world.query::<(&Position, &BuildSite)>().iter() {
            let (bw, bh) = site.building_type.size();
            let bx = pos.x as i32;
            let by = pos.y as i32;
            for dy in 0..bh {
                for dx in 0..bw {
                    let fx = bx + dx;
                    let fy = by + dy;
                    if fx >= 0 && fy >= 0 && (fx as usize) < w && (fy as usize) < h {
                        pending_blocked[fy as usize * w + fx as usize] = true;
                    }
                }
            }
        }

        let mut queue = std::collections::VecDeque::new();
        let start_idx = sj as usize * w + si as usize;
        visited[start_idx] = true;
        queue.push_back((si, sj));
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let idx = ny as usize * w + nx as usize;
                if visited[idx] || pending_blocked[idx] {
                    continue;
                }
                if (nx - si).abs() > radius || (ny - sj).abs() > radius {
                    continue;
                }
                if self.map.is_walkable(nx as f64, ny as f64) {
                    visited[idx] = true;
                    queue.push_back((nx, ny));
                }
            }
        }
        visited
    }

    /// Score a candidate building position based on terrain features.
    /// Returns a f64 score where higher is better. Different building types
    /// weight terrain features differently (farms prefer water, garrisons
    /// prefer high ground and chokepoints, huts cluster together, etc.).
    pub(super) fn score_building_spot(
        &self,
        bx: i32,
        by: i32,
        bt: BuildingType,
        cx: f64,
        cy: f64,
    ) -> f64 {
        let w = self.map.width;
        let h = self.map.height;
        let (bw, bh) = bt.size();

        // Use the center tile of the building footprint for scoring.
        let mx = bx + bw / 2;
        let my = by + bh / 2;
        if mx < 0 || my < 0 || mx as usize >= w || my as usize >= h {
            return f64::NEG_INFINITY;
        }
        let idx = my as usize * w + mx as usize;

        // ── Per-building-type weights ──
        // (water_prox, fertility, flatness, high_elev, chokepoint, cluster, stone_prox, wood_prox, traffic, dist_pen)
        let (
            w_water,
            w_fert,
            w_flat,
            w_high,
            w_choke,
            w_cluster,
            w_stone,
            w_wood,
            w_traffic,
            w_dist,
        ): (f64, f64, f64, f64, f64, f64, f64, f64, f64, f64) = match bt {
            BuildingType::Farm => (2.0, 3.0, 1.5, 0.0, 0.0, 0.3, 0.0, 0.0, 0.0, -0.5),
            BuildingType::Hut => (0.5, 0.0, 1.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, -1.0),
            BuildingType::Stockpile => (0.3, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.5, -0.3),
            BuildingType::Garrison => (0.0, 0.0, 0.5, 1.5, 3.0, 0.0, 0.0, 0.0, 0.0, -0.2),
            BuildingType::Wall => (0.0, 0.0, 0.3, 1.0, 2.5, 0.0, 0.0, 0.0, 0.0, -0.1),
            BuildingType::Workshop => (0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 1.5, 1.0, -1.0),
            BuildingType::Smithy => (0.0, 0.0, 1.0, 0.5, 0.0, 2.0, 1.5, 0.0, 1.0, -1.0),
            BuildingType::Granary => (0.0, 0.0, 1.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, -0.8),
            BuildingType::Bakery => (0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 1.0, -1.0),
            BuildingType::TownHall => (0.5, 0.0, 1.5, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, -0.5),
            BuildingType::Road => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0),
            BuildingType::Bridge => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        };

        let mut score = 0.0;

        // ── Water proximity: BFS-free approximation using river_mask ──
        // Scan outward from building center to find nearest water/river tile.
        if w_water.abs() > 0.01 {
            let mut min_water_dist = 30.0f64; // cap
            'water_search: for r in 0i32..30 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let wx = mx + dx;
                        let wy = my + dy;
                        if wx < 0 || wy < 0 || wx as usize >= w || wy as usize >= h {
                            continue;
                        }
                        let wi = wy as usize * w + wx as usize;
                        let is_water = self.river_mask.get(wi).copied().unwrap_or(false)
                            || matches!(
                                self.map.get(wx as usize, wy as usize),
                                Some(Terrain::Water)
                            );
                        if is_water {
                            min_water_dist = r as f64;
                            break 'water_search;
                        }
                    }
                }
            }
            score += w_water * (1.0 / (1.0 + min_water_dist * 0.15));
        }

        // ── Soil fertility (from ResourceMap) ──
        if w_fert.abs() > 0.01 {
            let fertility =
                self.resource_map.get(mx as usize, my as usize).fertility as f64 / 255.0;
            score += w_fert * fertility;
        }

        // ── Flatness: prefer low slope (approximate from height differences) ──
        if w_flat.abs() > 0.01 {
            if idx < self.heights.len() {
                let center_h = self.heights[idx];
                let mut max_diff = 0.0f64;
                for (dx, dy) in [(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
                    let nx = mx + dx;
                    let ny = my + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                        let ni = ny as usize * w + nx as usize;
                        if ni < self.heights.len() {
                            let diff = (self.heights[ni] - center_h).abs();
                            if diff > max_diff {
                                max_diff = diff;
                            }
                        }
                    }
                }
                // Flat: max_diff < 0.05 → score ~1.0; steep: max_diff > 0.1 → score ~0.0
                score += w_flat * (1.0 - (max_diff / 0.1).min(1.0));
            }
        }

        // ── Elevation (high ground for garrisons/walls) ──
        if w_high.abs() > 0.01 && idx < self.heights.len() {
            score += w_high * self.heights[idx];
        }

        // ── Chokepoint: how narrow the walkable corridor is here ──
        // Measure minimum clear distance to impassable terrain in 8 directions.
        if w_choke.abs() > 0.01 {
            let mut min_clear = 40i32;
            for (ddx, ddy) in [
                (1i32, 0),
                (-1, 0),
                (0, 1i32),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let mut dist = 0i32;
                loop {
                    dist += 1;
                    let sx = mx + ddx * dist;
                    let sy = my + ddy * dist;
                    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                        break;
                    }
                    if !self.map.is_walkable(sx as f64, sy as f64) {
                        break;
                    }
                    if dist >= 40 {
                        break;
                    }
                }
                if dist < min_clear {
                    min_clear = dist;
                }
            }
            // Narrow pass (min_clear=2) → 1/(2+1)=0.33; open field (min_clear=40) → 0.024
            score += w_choke * (1.0 / (min_clear as f64 + 1.0));
        }

        // ── Cluster bonus: count buildings of same type within 8 tiles ──
        if w_cluster.abs() > 0.01 {
            let mut nearby_count = 0u32;
            for (pos, _site) in self.world.query::<(&Position, &BuildSite)>().iter() {
                let dx = (pos.x as i32 - mx).abs();
                let dy = (pos.y as i32 - my).abs();
                if dx <= 8 && dy <= 8 {
                    nearby_count += 1;
                }
            }
            // Also count completed buildings via terrain tiles (BuildingFloor/Wall within 8 tiles)
            for dy in -8i32..=8 {
                for dx in -8i32..=8 {
                    let tx = mx + dx;
                    let ty = my + dy;
                    if tx >= 0 && ty >= 0 && (tx as usize) < w && (ty as usize) < h {
                        if matches!(
                            self.map.get(tx as usize, ty as usize),
                            Some(Terrain::BuildingFloor | Terrain::BuildingWall)
                        ) {
                            nearby_count += 1;
                        }
                    }
                }
            }
            score += w_cluster * (nearby_count as f64 * 0.1).min(0.5);
        }

        // ── Stone proximity (for smithy) ──
        if w_stone.abs() > 0.01 {
            let stone_pot = self.resource_map.get(mx as usize, my as usize).stone as f64 / 255.0;
            score += w_stone * stone_pot;
        }

        // ── Wood proximity (for workshop) ──
        if w_wood.abs() > 0.01 {
            let wood_pot = self.resource_map.get(mx as usize, my as usize).wood as f64 / 255.0;
            score += w_wood * wood_pot;
        }

        // ── Traffic / crossroads (for stockpiles, town halls) ──
        if w_traffic.abs() > 0.01 {
            let traffic_val = self.traffic.get(mx as usize, my as usize);
            // Normalize: traffic values vary widely; use a sigmoid-like curve.
            let normalized = traffic_val / (traffic_val + 50.0);
            score += w_traffic * normalized;
        }

        // ── Distance penalty: soft falloff from centroid ──
        let dist = ((bx as f64 - cx).powi(2) + (by as f64 - cy).powi(2)).sqrt();
        score += w_dist * (dist / 20.0);

        // ── Spacing penalty: discourage same-type buildings within 5 tiles ──
        let mut same_type_nearby = 0u32;
        for (pos, site) in self.world.query::<(&Position, &BuildSite)>().iter() {
            if site.building_type == bt {
                let dx = (pos.x as i32 - mx).abs();
                let dy = (pos.y as i32 - my).abs();
                if dx <= 5 && dy <= 5 {
                    same_type_nearby += 1;
                }
            }
        }
        // Check completed buildings too (for farms, huts, etc.)
        match bt {
            BuildingType::Farm => {
                for (pos, _) in self.world.query::<(&Position, &FarmPlot)>().iter() {
                    let dx = (pos.x as i32 - mx).abs();
                    let dy = (pos.y as i32 - my).abs();
                    if dx <= 5 && dy <= 5 {
                        same_type_nearby += 1;
                    }
                }
            }
            BuildingType::Hut => {
                for (pos, _) in self.world.query::<(&Position, &HutBuilding)>().iter() {
                    let dx = (pos.x as i32 - mx).abs();
                    let dy = (pos.y as i32 - my).abs();
                    if dx <= 5 && dy <= 5 {
                        same_type_nearby += 1;
                    }
                }
            }
            _ => {}
        }
        if same_type_nearby > 0 {
            score -= 0.3 * same_type_nearby as f64;
        }

        score
    }

    /// Find a valid spot for a building near (cx, cy) using terrain-aware scoring.
    /// Scans all reachable candidates within a search radius, scores each by terrain
    /// features appropriate to the building type, and returns the highest-scoring spot.
    /// Falls back to an expanded ring scan if no scored candidates are found.
    pub(super) fn find_building_spot(
        &self,
        cx: f64,
        cy: f64,
        bt: BuildingType,
    ) -> Option<(i32, i32)> {
        let (bw, bh) = bt.size();
        // Pre-compute which tiles are reachable from the settlement centroid.
        let reachable = self.reachable_tiles(cx, cy, 80);
        let is_reachable = |bx: i32, by: i32| -> bool {
            let rcx = bx + bw / 2;
            let rcy = by + bh / 2;
            if rcx < 0 || rcy < 0 {
                return false;
            }
            let idx = rcy as usize * self.map.width + rcx as usize;
            idx < reachable.len() && reachable[idx]
        };

        // ── Scored search: scan all candidates within search_radius, pick best ──
        let search_radius: i32 = match bt {
            BuildingType::Garrison => 40, // willing to be far out at a chokepoint
            BuildingType::Wall => 30,
            BuildingType::Farm => 25, // farms can be placed further out near water
            _ => 20,                  // huts, workshops, etc. stay close
        };

        let mut best_score = f64::NEG_INFINITY;
        let mut best_pos: Option<(i32, i32)> = None;

        // Coarse grid pass: building-size steps within search_radius
        let step_x = bw.max(1);
        let step_y = bh.max(1);
        let coarse_r = search_radius / step_x.max(step_y);
        for dy in -coarse_r..=coarse_r {
            for dx in -coarse_r..=coarse_r {
                let bx = cx as i32 + dx * step_x;
                let by = cy as i32 + dy * step_y;
                if self.can_place_building_impl(bx, by, bt, false) && is_reachable(bx, by) {
                    let s = self.score_building_spot(bx, by, bt, cx, cy);
                    if s > best_score {
                        best_score = s;
                        best_pos = Some((bx, by));
                    }
                }
            }
        }

        // Fine grid pass if coarse found nothing (narrow corridors)
        if best_pos.is_none() {
            for r in 1i32..search_radius {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue; // perimeter only for efficiency
                        }
                        let bx = cx as i32 + dx;
                        let by = cy as i32 + dy;
                        if self.can_place_building_impl(bx, by, bt, false) && is_reachable(bx, by) {
                            let s = self.score_building_spot(bx, by, bt, cx, cy);
                            if s > best_score {
                                best_score = s;
                                best_pos = Some((bx, by));
                            }
                        }
                    }
                }
                // Early exit: once we have candidates from a ring close to center,
                // don't scan all the way out. But ensure we have at least a few rings.
                if best_pos.is_some() && r >= 8 {
                    break;
                }
            }
        }

        // Fallback: expanded ring scan (original algorithm) for extreme terrain
        if best_pos.is_none() {
            for r in search_radius..64 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let bx = cx as i32 + dx;
                        let by = cy as i32 + dy;
                        if self.can_place_building_impl(bx, by, bt, false) && is_reachable(bx, by) {
                            return Some((bx, by));
                        }
                    }
                }
            }
        }

        best_pos
    }

    /// Find a spot for a defensive wall between settlement center and nearest wolf.
    pub(super) fn find_wall_spot(&self, cx: f64, cy: f64) -> Option<(i32, i32)> {
        // Find direction to nearest wolf
        let mut nearest_dist = f64::MAX;
        let mut wolf_dir = (0.0f64, 0.0f64);
        for (p, c) in self.world.query::<(&Position, &Creature)>().iter() {
            if c.species != Species::Predator {
                continue;
            }
            let dx = p.x - cx;
            let dy = p.y - cy;
            let dist = dx * dx + dy * dy;
            if dist < nearest_dist {
                nearest_dist = dist;
                let d = dist.sqrt().max(1.0);
                wolf_dir = (dx / d, dy / d);
            }
        }
        if nearest_dist == f64::MAX {
            return None;
        }
        // Place wall ~8 tiles out in that direction, searching nearby for valid spot
        let target_x = cx as i32 + (wolf_dir.0 * 8.0) as i32;
        let target_y = cy as i32 + (wolf_dir.1 * 8.0) as i32;
        for r in 0..5 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let wx = target_x + dx;
                    let wy = target_y + dy;
                    if self.can_place_building(wx, wy, BuildingType::Wall) {
                        return Some((wx, wy));
                    }
                }
            }
        }
        None
    }

    /// Demolish any building at (bx, by). Restores terrain to grass and despawns entity.
    pub(super) fn demolish_at(&mut self, bx: i32, by: i32) {
        // Find building entity at this position
        let mut to_demolish: Option<hecs::Entity> = None;
        let mut building_size = (1i32, 1i32);

        // Check for huts
        for (entity, (pos, _)) in self
            .world
            .query::<(hecs::Entity, (&Position, &HutBuilding))>()
            .iter()
        {
            let (w, h) = BuildingType::Hut.size();
            let ex = pos.x as i32 - w / 2;
            let ey = pos.y as i32 - h / 2;
            if bx >= ex && bx < ex + w && by >= ey && by < ey + h {
                to_demolish = Some(entity);
                building_size = (w, h);
                break;
            }
        }

        // Check for farms — also capture farm tile coords for fertility-based terrain scar
        let mut demolished_farm_tile: Option<(usize, usize)> = None;
        if to_demolish.is_none() {
            for (entity, (pos, farm)) in self
                .world
                .query::<(hecs::Entity, (&Position, &FarmPlot))>()
                .iter()
            {
                let (w, h) = BuildingType::Farm.size();
                let ex = pos.x as i32 - w / 2;
                let ey = pos.y as i32 - h / 2;
                if bx >= ex && bx < ex + w && by >= ey && by < ey + h {
                    to_demolish = Some(entity);
                    building_size = (w, h);
                    demolished_farm_tile = Some((farm.tile_x, farm.tile_y));
                    break;
                }
            }
        }

        // Check for garrisons
        if to_demolish.is_none() {
            for (entity, (pos, _)) in self
                .world
                .query::<(hecs::Entity, (&Position, &GarrisonBuilding))>()
                .iter()
            {
                let (w, h) = BuildingType::Garrison.size();
                let ex = pos.x as i32 - w / 2;
                let ey = pos.y as i32 - h / 2;
                if bx >= ex && bx < ex + w && by >= ey && by < ey + h {
                    to_demolish = Some(entity);
                    building_size = (w, h);
                    break;
                }
            }
        }

        // Check for town hall
        if to_demolish.is_none() {
            for (entity, (pos, _)) in self
                .world
                .query::<(hecs::Entity, (&Position, &TownHallBuilding))>()
                .iter()
            {
                let (w, h) = BuildingType::TownHall.size();
                let ex = pos.x as i32 - w / 2;
                let ey = pos.y as i32 - h / 2;
                if bx >= ex && bx < ex + w && by >= ey && by < ey + h {
                    to_demolish = Some(entity);
                    building_size = (w, h);
                    break;
                }
            }
        }

        // Check for processing buildings (workshop, smithy, bakery)
        if to_demolish.is_none() {
            for (entity, (pos, _)) in self
                .world
                .query::<(hecs::Entity, (&Position, &ProcessingBuilding))>()
                .iter()
            {
                let (w, h) = (3, 3); // processing buildings are 3x3
                let ex = pos.x as i32 - w / 2;
                let ey = pos.y as i32 - h / 2;
                if bx >= ex && bx < ex + w && by >= ey && by < ey + h {
                    to_demolish = Some(entity);
                    building_size = (w, h);
                    break;
                }
            }
        }

        // Check for build sites (in-progress buildings)
        if to_demolish.is_none() {
            for (entity, (pos, site)) in self
                .world
                .query::<(hecs::Entity, (&Position, &BuildSite))>()
                .iter()
            {
                let (w, h) = site.building_type.size();
                let ex = pos.x as i32;
                let ey = pos.y as i32;
                if bx >= ex && bx < ex + w && by >= ey && by < ey + h {
                    to_demolish = Some(entity);
                    building_size = (w, h);
                    break;
                }
            }
        }

        if let Some(entity) = to_demolish {
            // Check if this was an exhausted farm — leave Sand scar if fertility < 0.2
            let exhausted_farm =
                demolished_farm_tile.is_some_and(|(fx, fy)| self.soil_fertility.get(fx, fy) < 0.2);

            let _ = self.world.despawn(entity);
            // Restore terrain under demolished building
            // Exhausted farms leave Sand (visible scar), others revert to Grass
            let restore_terrain = if exhausted_farm {
                Terrain::Sand
            } else {
                Terrain::Grass
            };
            for dy in 0..building_size.1 {
                for dx in 0..building_size.0 {
                    let tx = bx + dx;
                    let ty = by + dy;
                    if tx >= 0 && ty >= 0 {
                        let tux = tx as usize;
                        let tuy = ty as usize;
                        if let Some(t) = self.map.get(tux, tuy)
                            && matches!(t, Terrain::BuildingFloor | Terrain::BuildingWall)
                        {
                            self.map.set(tux, tuy, restore_terrain);
                        }
                    }
                }
            }
            if exhausted_farm {
                self.notify("Exhausted farm demolished — soil scarred.".to_string());
            } else {
                self.notify("Building demolished.".to_string());
            }
        }
    }

    /// Recompute settlement knowledge: known resource locations and frontier tiles.
    pub(super) fn update_settlement_knowledge(&mut self) {
        let villager_pos: Vec<(f64, f64)> = self
            .world
            .query::<(&Position, &Creature)>()
            .iter()
            .filter(|(_, c)| c.species == Species::Villager)
            .map(|(p, _)| (p.x, p.y))
            .collect();
        if villager_pos.is_empty() {
            return;
        }
        let cx = villager_pos.iter().map(|p| p.0).sum::<f64>() / villager_pos.len() as f64;
        let cy = villager_pos.iter().map(|p| p.1).sum::<f64>() / villager_pos.len() as f64;
        let cxi = cx as i32;
        let cyi = cy as i32;

        let known_radius = 20i32;
        let frontier_radius = known_radius + 8;

        let mut known_wood = Vec::new();
        let mut known_stone = Vec::new();
        let mut known_food = Vec::new();
        let mut frontier = Vec::new();

        let w = self.map.width as i32;
        let h = self.map.height as i32;

        for dy in -known_radius..=known_radius {
            for dx in -known_radius..=known_radius {
                let tx = cxi + dx;
                let ty = cyi + dy;
                if tx < 0 || ty < 0 || tx >= w || ty >= h {
                    continue;
                }
                let ux = tx as usize;
                let uy = ty as usize;
                if let Some(terrain) = self.map.get(ux, uy) {
                    match terrain {
                        Terrain::Forest => known_wood.push((ux, uy)),
                        Terrain::Mountain => known_stone.push((ux, uy)),
                        _ => {}
                    }
                }
            }
        }

        for (pos, _) in self
            .world
            .query::<(&Position, &crate::ecs::FoodSource)>()
            .iter()
        {
            let d = ((pos.x - cx).powi(2) + (pos.y - cy).powi(2)).sqrt();
            if d < known_radius as f64 {
                known_food.push((pos.x as usize, pos.y as usize));
            }
        }

        for (pos, _) in self
            .world
            .query::<(&Position, &crate::ecs::StoneDeposit)>()
            .iter()
        {
            let d = ((pos.x - cx).powi(2) + (pos.y - cy).powi(2)).sqrt();
            if d < known_radius as f64 {
                known_stone.push((pos.x as usize, pos.y as usize));
            }
        }

        for dy in -frontier_radius..=frontier_radius {
            for dx in -frontier_radius..=frontier_radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < known_radius * known_radius
                    || dist_sq > frontier_radius * frontier_radius
                {
                    continue;
                }
                let tx = cxi + dx;
                let ty = cyi + dy;
                if tx < 0 || ty < 0 || tx >= w || ty >= h {
                    continue;
                }
                if self.map.is_walkable(tx as f64, ty as f64) {
                    frontier.push((tx as usize, ty as usize));
                }
            }
        }

        self.knowledge = super::SettlementKnowledge {
            known_wood,
            known_stone,
            known_food,
            frontier,
        };
    }
}
