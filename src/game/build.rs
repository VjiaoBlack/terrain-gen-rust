use super::{CELL_ASPECT, PANEL_WIDTH, ROAD_TRAFFIC_THRESHOLD};
#[allow(unused_imports)] // some used only in demolish_at
use crate::ecs::{
    self, BuildSite, BuildingType, Creature, FarmPlot, GarrisonBuilding, HutBuilding, Position,
    ProcessingBuilding, Recipe, Species, Stockpile,
};
use crate::renderer::Renderer;
use crate::tilemap::Terrain;

impl super::Game {
    pub fn can_place_building(&self, bx: i32, by: i32, building_type: BuildingType) -> bool {
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
                    match terrain {
                        Terrain::Grass
                        | Terrain::Sand
                        | Terrain::Forest
                        | Terrain::Scrubland
                        | Terrain::Desert
                        | Terrain::Tundra => {} // ok
                        _ => return false, // water, mountain, snow, cliff, marsh, buildings
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

        // Must be within settlement influence (any tile of building footprint)
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

        for dy in 0..sh {
            for dx in 0..sw {
                let tx = bx + dx;
                let ty = by + dy;
                if tx >= 0 && ty >= 0 {
                    self.map
                        .set(tx as usize, ty as usize, Terrain::BuildingFloor);
                }
            }
        }
        ecs::spawn_build_site(&mut self.world, bx as f64, by as f64, bt);
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
            if site.building_type == BuildingType::Granary {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::FoodToGrain);
            }
            if site.building_type == BuildingType::Bakery {
                ecs::spawn_processing_building(&mut self.world, pos.x, pos.y, Recipe::GrainToBread);
            }
            self.world.despawn(e).ok();
        }

        // Push any creatures stuck in walls to nearest walkable tile
        if !completed.is_empty() {
            let mut to_move: Vec<(hecs::Entity, f64, f64)> = Vec::new();
            for (entity, (pos, _creature)) in self
                .world
                .query::<(hecs::Entity, (&Position, &Creature))>()
                .iter()
            {
                let ix = pos.x.round() as usize;
                let iy = pos.y.round() as usize;
                if !self.map.get(ix, iy).map_or(false, |t| t.is_walkable()) {
                    // Find nearest walkable tile
                    for r in 1..10i32 {
                        let mut found = false;
                        for dy in -r..=r {
                            for dx in -r..=r {
                                if dx.abs() != r && dy.abs() != r {
                                    continue;
                                }
                                let nx = pos.x + dx as f64;
                                let ny = pos.y + dy as f64;
                                if self.map.is_walkable(nx, ny) {
                                    to_move.push((entity, nx, ny));
                                    found = true;
                                    break;
                                }
                            }
                            if found {
                                break;
                            }
                        }
                        if found {
                            break;
                        }
                    }
                }
            }
            for (entity, nx, ny) in to_move {
                if let Ok(mut pos) = self.world.get::<&mut Position>(entity) {
                    pos.x = nx;
                    pos.y = ny;
                }
            }
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

        // Count total hut capacity
        let total_capacity: u32 = self
            .world
            .query::<&HutBuilding>()
            .iter()
            .map(|h| h.capacity)
            .sum();

        // Housing surplus determines birth rate
        let housing_surplus = total_capacity.saturating_sub(villager_count);
        let birth_cooldown = if housing_surplus == 0 {
            return; // No births without housing surplus
        } else if housing_surplus >= 4 {
            200 // Fast growth when lots of empty housing
        } else {
            800 / housing_surplus as u64 // 800, 400, 266 for surplus 1, 2, 3
        };

        if self.tick - self.last_birth_tick <= birth_cooldown {
            return;
        }

        if villager_count < 2 || self.resources.food < 5 {
            return;
        }

        self.resources.food -= 5;

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
            ecs::spawn_villager(&mut self.world, nx, ny);
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

        // Count existing farms (completed + in-progress)
        let farm_count = self.world.query::<&FarmPlot>().iter().count()
            + self
                .world
                .query::<&BuildSite>()
                .iter()
                .filter(|s| s.building_type == BuildingType::Farm)
                .count();

        // Count existing build sites being worked on
        let pending_builds = self.world.query::<&BuildSite>().iter().count();
        // Don't queue too many builds at once
        if pending_builds >= 3 {
            return;
        }

        // Priority 1: Farm when food is low and we don't have many farms
        let villager_count = villager_pos.len() as u32;
        if self.resources.food < 8 + villager_count * 2
            && farm_count < (villager_count as usize).div_ceil(2)
        {
            let cost = BuildingType::Farm.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Farm)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Farm);
                self.notify("Auto-build: Farm queued".to_string());
                return;
            }
        }

        // Priority 2: Hut when population is growing and needs housing
        let hut_count = self
            .world
            .query::<&BuildSite>()
            .iter()
            .filter(|s| s.building_type == BuildingType::Hut)
            .count();
        // Count completed huts by checking for Hut-shaped building floor clusters
        // Simple heuristic: 1 hut per 3 villagers needed
        let huts_needed = (villager_count as usize).div_ceil(3);
        if hut_count < huts_needed && villager_count >= 3 {
            let cost = BuildingType::Hut.cost();
            if self.resources.can_afford(&cost)
                && let Some((bx, by)) = self.find_building_spot(cx, cy, BuildingType::Hut)
            {
                self.resources.deduct(&cost);
                self.place_build_site(bx, by, BuildingType::Hut);
                self.notify("Auto-build: Hut queued".to_string());
                return;
            }
        }

        // Priority 3: Walls when wolves are nearby settlement
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

    /// Find a valid spot for a building near (cx, cy), searching outward in rings.
    pub(super) fn find_building_spot(
        &self,
        cx: f64,
        cy: f64,
        bt: BuildingType,
    ) -> Option<(i32, i32)> {
        let (bw, bh) = bt.size();
        let is_farm = bt == BuildingType::Farm;
        let mut best: Option<(i32, i32, f64)> = None;

        for r in 2i32..20 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let bx = cx as i32 + dx * bw;
                    let by = cy as i32 + dy * bh;
                    if self.can_place_building(bx, by, bt) {
                        // Score by soil fertility
                        let soil_idx = by as usize * self.map.width + bx as usize;
                        let fertility = if soil_idx < self.soil.len() {
                            self.soil[soil_idx].yield_multiplier()
                        } else {
                            1.0
                        };
                        // Farms prefer high fertility, others prefer low
                        let score = if is_farm { fertility } else { 2.0 - fertility };
                        if best.is_none() || score > best.unwrap().2 {
                            best = Some((bx, by, score));
                        }
                    }
                }
            }
            // If we found any candidate in this ring, use the best one
            if best.is_some() {
                break;
            }
        }
        best.map(|(bx, by, _)| (bx, by))
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

        // Check for farms
        if to_demolish.is_none() {
            for (entity, (pos, _)) in self
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
            let _ = self.world.despawn(entity);
            // Restore terrain under demolished building to grass
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
                            self.map.set(tux, tuy, Terrain::Grass);
                        }
                    }
                }
            }
            self.notify("Building demolished.".to_string());
        }
    }
}
