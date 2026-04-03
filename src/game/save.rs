use super::{Game, OverlayMode, SaveState};
use crate::ecs::{self, BuildingType};
use crate::simulation::ExplorationMap;
use crate::tilemap::Camera;
use anyhow::Result;

impl Game {
    pub fn save(&self, path: &str) -> Result<()> {
        let state = SaveState {
            tick: self.tick,
            resources: self.resources.clone(),
            skills: self.skills.clone(),
            day_night: serde_json::from_value(serde_json::to_value(&self.day_night)?)?,
            map: serde_json::from_value(serde_json::to_value(&self.map)?)?,
            heights: self.heights.clone(),
            water: serde_json::from_value(serde_json::to_value(&self.water)?)?,
            moisture: serde_json::from_value(serde_json::to_value(&self.moisture)?)?,
            vegetation: serde_json::from_value(serde_json::to_value(&self.vegetation)?)?,
            influence: serde_json::from_value(serde_json::to_value(&self.influence)?)?,
            entities: ecs::serialize_world(&self.world),
            last_birth_tick: self.last_birth_tick,
            peak_population: self.peak_population,
            raining: self.raining,
            auto_build: self.auto_build,
            sim_config: self.sim_config.clone(),
            terrain_config: serde_json::from_value(serde_json::to_value(&self.terrain_config)?)?,
            events: self.events.clone(),
            traffic: serde_json::from_value(serde_json::to_value(&self.traffic)?)?,
            danger_scent: serde_json::from_value(serde_json::to_value(&self.danger_scent)?)?,
            home_scent: serde_json::from_value(serde_json::to_value(&self.home_scent)?)?,
            resource_map: Some(self.resource_map.clone()),
        };
        let file = std::fs::File::create(path)?;
        serde_json::to_writer(file, &state)?;
        Ok(())
    }

    pub fn load(path: &str, target_fps: u32) -> Result<Game> {
        let file = std::fs::File::open(path)?;
        let state: SaveState = serde_json::from_reader(file)?;
        let map_w = state.map.width;
        let map_h = state.map.height;
        let mut game = Game {
            target_fps,
            tick: state.tick,
            map: state.map,
            heights: state.heights,
            water: state.water,
            moisture: state.moisture,
            vegetation: state.vegetation,
            sim_config: state.sim_config,
            terrain_config: state.terrain_config,
            camera: Camera { x: 0, y: 0 },
            world: ecs::deserialize_world(&state.entities),
            day_night: state.day_night,
            scroll_speed: 2,
            raining: state.raining,
            render_mode: super::RenderMode::Normal,
            paused: false,
            query_mode: false,
            query_cx: 0,
            query_cy: 0,
            display_fps: None,
            resources: state.resources,
            build_mode: false,
            build_cursor_x: 0,
            build_cursor_y: 0,
            selected_building: BuildingType::Wall,
            influence: state.influence,
            last_birth_tick: state.last_birth_tick,
            notifications: vec![],
            game_over: false,
            peak_population: state.peak_population,
            auto_build: state.auto_build,
            skills: state.skills,
            overlay: OverlayMode::None,
            events: state.events,
            traffic: state.traffic,
            danger_scent: if state.danger_scent.width > 0 {
                state.danger_scent
            } else {
                crate::simulation::ScentMap::new(map_w, map_h, 0.990, 0.06)
            },
            home_scent: if state.home_scent.width > 0 {
                state.home_scent
            } else {
                crate::simulation::ScentMap::new(map_w, map_h, 0.998, 0.08)
            },
            exploration: ExplorationMap::new(map_w, map_h),
            particles: Vec::new(),
            game_speed: 1,
            frame_count: 0,
            half_speed_base: false,
            soil_fertility: crate::simulation::SoilFertilityMap::new(map_w, map_h),
            soil: vec![crate::terrain_pipeline::SoilType::Loam; map_w * map_h],
            river_mask: vec![false; map_w * map_h],
            pipeline_temperature: vec![0.5; map_w * map_h],
            pipeline_slope: vec![0.0; map_w * map_h],
            pipeline_moisture: vec![0.5; map_w * map_h],
            resource_map: state
                .resource_map
                .unwrap_or_else(|| crate::terrain_pipeline::ResourceMap::new(map_w, map_h)),
            knowledge: super::SettlementKnowledge::default(),
            spatial_grid: crate::ecs::spatial::SpatialHashGrid::new(map_w, map_h, 16),
            group_manager: crate::ecs::groups::GroupManager::new(),
            ai_arrays: crate::ecs::ai_arrays::AiArrays::new(64),
            difficulty: super::DifficultyState::default(),
            milestone_banner: None,
            flood_start_tick: 0,
            flooded_tiles: Vec::new(),
            raid_survived_clean: false,
            fire_tiles: Vec::new(),
            chokepoint_map: super::chokepoint::ChokepointMap::empty(map_w, map_h),
            chokepoints_dirty: true, // recompute after load
            dirty: super::dirty::DirtyMap::new(map_w, map_h),
            prev_camera_x: i32::MIN,
            prev_camera_y: i32::MIN,
            flow_fields: crate::pathfinding::FlowFieldRegistry::new(),
            terrain_dirty_tick: 0,
            nav_graph: crate::pathfinding::NavGraph::default(), // rebuilt below
            threat_map: crate::simulation::ThreatMap::new(map_w, map_h),
            threat_score: 0.0,
            last_threat_tick: 0,
            outposts: Vec::new(),
            #[cfg(feature = "lua")]
            script_engine: None,
        };
        // Recompute chokepoint map from loaded terrain
        game.chokepoint_map =
            super::chokepoint::ChokepointMap::compute(&game.map, &game.river_mask);
        game.chokepoints_dirty = false;
        game.nav_graph = crate::pathfinding::NavGraph::build(&game.map);
        Ok(game)
    }
}
