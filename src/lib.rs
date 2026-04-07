pub mod analytical_erosion;
// TODO: Migrate Game to use WorldState as the single source of truth.
// Currently Game holds these fields directly. WorldState is defined
// but not yet wired in — see docs/design/cross_cutting/state_driven_architecture.md
pub mod world_state;
pub mod ecs;
pub mod game;
pub mod headless_renderer;
pub mod hydrology;
pub mod pathfinding;
pub mod pipe_water;
pub mod renderer;
#[cfg(feature = "lua")]
pub mod scripting;
pub mod simulation;
pub mod terrain_gen;
pub mod terrain_pipeline;
pub mod tilemap;
