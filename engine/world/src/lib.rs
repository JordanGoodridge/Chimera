//! The engine's map (docs/ARCHITECTURE.md, "Crates"): the neutral map data
//! any game's importer produces, collision, spawning a map's simulated
//! parts and the map's systems (`map`). It uses only the engine's base
//! (`engine_core`) and the formats of ours; what draws and plays the map
//! (`engine_present`) sits above it, so an edit to drawing doesn't
//! rebuild the simulation's map, and the other way round.
//!
//! The game crate re-exports `map` at its old path (`mashup::map`), with
//! `MapPlugin`, whose registration lives above (`map::build_first` and its
//! siblings are map's part of it).

pub mod map;

// `crate::core` inside `map`, as in the game crate.
use engine_core::core;
