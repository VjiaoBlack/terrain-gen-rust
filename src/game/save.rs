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
        Ok(Game {
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
            debug_view: false,
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
            exploration: ExplorationMap::new(map_w, map_h),
            particles: Vec::new(),
            game_speed: 1,
            difficulty: super::DifficultyState::default(),
            #[cfg(feature = "lua")]
            script_engine: None,
        })
    }
}
