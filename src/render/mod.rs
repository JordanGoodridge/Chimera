//! Presentation of the map: what draws the parts `map` spawns and
//! simulates. Nothing in the simulation uses it (`tests/it/architecture.rs`);
//! `map_plugin` builds it into `map::MapPlugin`; only `map_plugin` and
//! `client` use it.
//!
//! So far the drawn phases of spawning a map (`spawn`, through
//! `map::MapDrawer`) and the effects' drawing; materials, culling and
//! views still live in `map` (docs/plans/active/engine-split.md, "Crates
//! and compile time"). `unload_map` is render's part of `map::unload_map`.

use bevy::prelude::*;

pub mod beams;
pub mod color_correction;
pub mod detail;
pub mod fish;
pub mod glow;
pub mod lod;
pub mod psys;
pub mod rope_sway;
pub mod shells;
pub mod sky_occluder;
mod spawn;
pub mod tint;
pub use spawn::{Drawn, MapAssets};

/// What `map::unload_map` removes of render's: its resources and the
/// entities it made (a `map::MapHooks::unload` hook, added by
/// `map_plugin`).
pub(crate) fn unload_map(world: &mut World) {
    world.remove_resource::<psys::ParticleSystemDefs>();
    shells::unload(world);
}
