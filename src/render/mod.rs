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
pub mod decal;
pub mod detail;
pub mod emitters;
pub mod fog;
pub mod dust;
pub mod fish;
pub mod glow;
pub mod lod;
pub mod monitor;
pub mod particles;
pub mod parts;
pub mod psys;
pub mod rope_sway;
pub mod shadows;
pub mod shells;
pub mod sky_occluder;
pub mod steam;
mod spawn;
pub mod tint;
pub mod tracer;
pub mod water;
pub use spawn::{Drawn, MapAssets};

/// What `map::unload_map` removes of render's: its resources and the
/// entities it made (a `map::MapHooks::unload` hook, added by
/// `map_plugin`).
pub(crate) fn unload_map(world: &mut World) {
    world.remove_resource::<decal::DecalSurfaces>();
    world.remove_resource::<decal::DecalAssets>();
    world.remove_resource::<decal::ImpactDecals>();
    world.remove_resource::<psys::ParticleSystemDefs>();
    world.remove_resource::<monitor::MonitorScreen>();
    world.remove_resource::<water::MapWaterRender>();
    world.remove_resource::<shadows::ShadowState>();
    world.remove_resource::<shadows::ShadowSource>();
    world.remove_resource::<particles::ParticleAssets>();
    shells::unload(world);
}
