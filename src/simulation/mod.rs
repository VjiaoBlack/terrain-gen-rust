use serde::{Deserialize, Serialize};

pub mod day_night;
pub mod maps;
pub mod moisture;
pub mod scent;
pub mod soil_fertility;
pub mod traffic;
pub mod vegetation;
pub mod water_map;
pub mod wind;

// Re-export everything so external code using `crate::simulation::Foo` still works.
pub use day_night::{DayNightCycle, Season, SeasonModifiers};
pub use maps::{ExplorationMap, InfluenceMap, ThreatMap};
pub use moisture::MoistureMap;
pub use scent::ScentMap;
pub use soil_fertility::SoilFertilityMap;
pub use traffic::TrafficMap;
pub use vegetation::VegetationMap;
pub use water_map::WaterMap;
pub use wind::WindField;

/// Which wind model to use for atmospheric simulation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WindModel {
    /// Jos Stam stable fluids solver. Physically accurate but static within a season.
    /// Good for: steady-state wind patterns, precise terrain deflection.
    Stam,
    /// Curl noise vector field with prevailing bias. Evolves every tick.
    /// Cheaper than Stam, produces natural swirling/mixing, better moisture distribution.
    /// Terrain modulates the noise field (high terrain dampens + deflects).
    CurlNoise,
}

impl Default for WindModel {
    fn default() -> Self {
        WindModel::CurlNoise
    }
}

/// Rain mode: how precipitation is generated.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RainMode {
    /// Wind-carried moisture from ocean evaporation (realistic).
    WindDriven,
    /// Uniform rain everywhere — bypasses wind moisture transport.
    Uniform,
    /// No rain at all — useful for isolating other systems.
    Off,
}

impl Default for RainMode {
    fn default() -> Self {
        RainMode::WindDriven
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub rain_rate: f64,     // fraction of tiles that get rain per tick
    pub rain_amount: f64,   // water added per raindrop
    pub flow_fraction: f64, // how much of height diff flows per tick
    pub evaporation: f64,   // water removed per tile per tick
    pub erosion_enabled: bool,
    pub erosion_strength: f64, // multiplier for erosion effect
    pub avg_factor: f64,       // smoothing: 0.95 = slow, 0.5 = fast
    #[serde(default)]
    pub wind_model: WindModel,
    #[serde(default)]
    pub rain_mode: RainMode,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            rain_rate: 0.03,
            rain_amount: 0.005,
            flow_fraction: 0.5,
            evaporation: 0.00001,
            erosion_enabled: false,
            erosion_strength: 1.0,
            avg_factor: 0.8, // faster averaging for responsive water visuals
            wind_model: WindModel::default(),
            rain_mode: RainMode::default(),
        }
    }
}
