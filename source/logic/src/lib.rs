//! Source's map logic (docs/ARCHITECTURE.md, "Crates"): entity I/O, the
//! logic classes, triggers, movers, breakables, props and fire, and the
//! bridge that runs them on the map's entities (`logic`). Any game whose
//! maps carry Source-style entities feeds it through
//! `map::MapData::entities`; `logic::net` is its part of the network
//! (movers, HUD messages). It uses the engine's base (`engine_core`), the
//! simulation's map (`engine_world`) and the network (`engine_net`) and
//! nothing above them, so an edit in the game crate above doesn't rebuild
//! it.
//!
//! The game crate re-exports `logic` at its old path (`mashup::logic`).

pub mod logic;

// `crate::core`, `crate::console`, `crate::map` and `crate::net` inside
// `logic`, as in the game crate.
use engine_core::{console, core};
use engine_net::net;
use engine_world::map;
