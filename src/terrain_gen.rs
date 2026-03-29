use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

use crate::tilemap::{Terrain, TileMap};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerrainGenConfig {
    pub seed: u32,
    pub scale: f64,
    pub octaves: usize,
    pub persistence: f64,
    pub lacunarity: f64,
    // height thresholds (0.0 to 1.0)
    pub water_level: f64,
    pub sand_level: f64,
    pub grass_level: f64,
    pub forest_level: f64,
    pub mountain_level: f64,
    // above mountain_level = snow
}

impl Default for TerrainGenConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            scale: 0.02,
            octaves: 4,
            persistence: 0.5,
            lacunarity: 2.0,
            water_level: 0.35,
            sand_level: 0.40,
            grass_level: 0.55,
            forest_level: 0.70,
            mountain_level: 0.85,
        }
    }
}

pub fn fbm(perlin: &Perlin, x: f64, y: f64, config: &TerrainGenConfig) -> f64 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut max_amplitude = 0.0;

    for _ in 0..config.octaves {
        value +=
            amplitude * perlin.get([x * frequency * config.scale, y * frequency * config.scale]);
        max_amplitude += amplitude;
        amplitude *= config.persistence;
        frequency *= config.lacunarity;
    }

    // normalize from [-1, 1] to [0, 1]
    (value / max_amplitude + 1.0) / 2.0
}

pub fn height_to_terrain(h: f64, config: &TerrainGenConfig) -> Terrain {
    if h < config.water_level {
        Terrain::Water
    } else if h < config.sand_level {
        Terrain::Sand
    } else if h < config.grass_level {
        Terrain::Grass
    } else if h < config.forest_level {
        Terrain::Forest
    } else if h < config.mountain_level {
        Terrain::Mountain
    } else {
        Terrain::Snow
    }
}

/// Returns (TileMap, height_data) where height_data is the raw f64 heights for simulation.
pub fn generate_terrain(
    width: usize,
    height: usize,
    config: &TerrainGenConfig,
) -> (TileMap, Vec<f64>) {
    let perlin = Perlin::new(config.seed);
    let mut map = TileMap::new(width, height, Terrain::Grass);
    let mut heights = vec![0.0f64; width * height];

    for y in 0..height {
        for x in 0..width {
            let h = fbm(&perlin, x as f64, y as f64, config);
            heights[y * width + x] = h;
            map.set(x, y, height_to_terrain(h, config));
        }
    }

    (map, heights)
}

/// Rebuild tile types from heights (call after erosion modifies terrain).
pub fn rebuild_tiles(map: &mut TileMap, heights: &[f64], config: &TerrainGenConfig) {
    for y in 0..map.height {
        for x in 0..map.width {
            let h = heights[y * map.width + x];
            map.set(x, y, height_to_terrain(h, config));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_correct_dimensions() {
        let (map, _) = generate_terrain(50, 30, &TerrainGenConfig::default());
        assert_eq!(map.width, 50);
        assert_eq!(map.height, 30);
    }

    #[test]
    fn all_tiles_are_valid() {
        let (map, _) = generate_terrain(100, 100, &TerrainGenConfig::default());
        for y in 0..100 {
            for x in 0..100 {
                assert!(map.get(x, y).is_some());
            }
        }
    }

    #[test]
    fn same_seed_produces_same_map() {
        let config = TerrainGenConfig::default();
        let (map1, _) = generate_terrain(50, 50, &config);
        let (map2, _) = generate_terrain(50, 50, &config);

        for y in 0..50 {
            for x in 0..50 {
                assert_eq!(map1.get(x, y), map2.get(x, y));
            }
        }
    }

    #[test]
    fn different_seeds_produce_different_maps() {
        let mut config1 = TerrainGenConfig::default();
        config1.seed = 1;
        let mut config2 = TerrainGenConfig::default();
        config2.seed = 999;

        let (map1, _) = generate_terrain(50, 50, &config1);
        let (map2, _) = generate_terrain(50, 50, &config2);

        let mut diffs = 0;
        for y in 0..50 {
            for x in 0..50 {
                if map1.get(x, y) != map2.get(x, y) {
                    diffs += 1;
                }
            }
        }
        assert!(
            diffs > 0,
            "different seeds should produce different terrain"
        );
    }

    #[test]
    fn generates_multiple_terrain_types() {
        let (map, _) = generate_terrain(200, 200, &TerrainGenConfig::default());
        let mut has_water = false;
        let mut has_grass = false;
        let mut has_forest = false;
        let mut has_mountain = false;

        for y in 0..200 {
            for x in 0..200 {
                match map.get(x, y).unwrap() {
                    Terrain::Water => has_water = true,
                    Terrain::Grass => has_grass = true,
                    Terrain::Forest => has_forest = true,
                    Terrain::Mountain => has_mountain = true,
                    _ => {}
                }
            }
        }

        assert!(has_water, "should generate water");
        assert!(has_grass, "should generate grass");
        assert!(has_forest, "should generate forest");
        assert!(has_mountain, "should generate mountain");
    }

    #[test]
    fn renders_to_headless() {
        use crate::headless_renderer::HeadlessRenderer;
        use crate::tilemap::{Camera, render_map};

        let (map, _) = generate_terrain(100, 100, &TerrainGenConfig::default());
        let camera = Camera::new(0, 0);
        let mut r = HeadlessRenderer::new(40, 20);
        render_map(&map, &camera, &mut r);

        let frame = r.frame_as_string();
        // should contain at least some non-space characters
        assert!(frame.chars().any(|c| c != ' ' && c != '\n'));
    }
}
