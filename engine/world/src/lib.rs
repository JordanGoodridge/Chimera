//! The engine's map (docs/ARCHITECTURE.md, "Crates"): the neutral map data
//! any game's importer produces, collision, spawning a map's simulated
//! parts and the map's systems (`map`). It uses only the engine's base
//! (`engine_core`) and the formats of ours; what draws and plays the map
//! (`engine_present`) sits above it, so an edit to drawing doesn't
//! rebuild the simulation's map, and the other way round.
//!
//! `map::MapSimPlugin` registers the map's systems alone (the dedicated
//! server's); `engine_present`'s `MapPlugin` registers the same with what
//! draws and plays the map at fixed places among them (`map::build`,
//! `map::MapLayers`). The game crate re-exports `map` at its old path
//! (`mashup::map`), with `MapPlugin` when it has the presentation.

pub mod map;

// `crate::core` inside `map`, as in the game crate.
use engine_core::core;
