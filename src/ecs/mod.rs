mod ai;
pub mod ai_arrays;
pub mod components;
pub mod groups;
pub mod serialize;
pub mod spatial;
pub mod spawn;
pub mod systems;

// Re-export everything so existing code using `crate::ecs::*` still works
pub use components::*;
pub use serialize::*;
pub use spawn::*;
pub use systems::*;

#[cfg(test)]
mod tests;
