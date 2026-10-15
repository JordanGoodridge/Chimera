//! The engine's presentation (docs/ARCHITECTURE.md, "Crates"): what draws
//! the map and what it simulates (`render`: the drawn phases of spawning,
//! drawn-part requests fulfilled, effects, culling, water, shadows, sky,
//! view models, materials and shaders) and what plays its sounds
//! (`audio`), beside the simulation and above it: it uses the map
//! (`engine_world`) and the engine's base, and nothing of the simulation
//! uses it. An edit here doesn't rebuild the map, and an edit in the
//! simulation above the map (`mashup`) doesn't rebuild this.
//!
//! `MapPlugin` (`map_plugin`) registers map's systems (`map::build`) with
//! audio's and render's at the places map keeps for them
//! (`map::MapLayers`), in one fixed order. The game crate re-exports
//! `render` and `audio` at their old paths and `MapPlugin` as
//! `mashup::map::MapPlugin`, when built with its `present` feature (the
//! dedicated server is built without this crate: `map::MapSimPlugin`).

pub mod audio;
mod map_plugin;
pub mod render;

pub use map_plugin::MapPlugin;

// `crate::core` and `crate::map` inside, as in the game crate.
use engine_core::core;
use engine_world::map;
