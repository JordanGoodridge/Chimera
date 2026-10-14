//! Presentation of the map: what draws the parts `map` spawns and
//! simulates. Nothing in the simulation uses it (`tests/it/architecture.rs`);
//! `map_plugin` builds it into `map::MapPlugin`.
//!
//! So far the drawn phases of spawning a map (`spawn`, through
//! `map::MapDrawer`); materials, culling, effects and views still live in
//! `map` (docs/plans/active/engine-split.md, "Crates and compile time").

mod spawn;
pub use spawn::{Drawn, MapAssets};
