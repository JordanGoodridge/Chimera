//! Presentation of the map: what draws the parts `map` spawns and
//! simulates. Nothing in the simulation uses it (`tests/it/architecture.rs`);
//! `map_plugin` builds it into `map::MapPlugin`; only `map_plugin` and
//! `client` use it.
//!
//! The drawn phases of spawning a map (`spawn`, through `map::MapDrawer`),
//! the effects, decals, water, shadows, culling, sky, light styles, monitor
//! screens and view models; the materials and their shaders, and the drawn
//! parts of bodies, breakables, loose items and ragdolls, still live in
//! `map` (docs/plans/active/engine-split.md, the render slice).
//! `unload_map` is render's part of `map::unload_map`.

use bevy::prelude::*;

pub mod beams;
pub mod bodies;
pub mod color_correction;
pub mod decal;
pub mod detail;
pub mod emitters;
pub mod fog;
pub mod dust;
pub mod fish;
pub mod glow;
pub mod light_styles;
pub mod lod;
pub mod monitor;
pub mod particles;
pub mod parts;
pub mod psys;
pub mod rope_sway;
pub mod shadows;
pub mod shells;
pub mod sky;
pub mod sky_occluder;
pub mod steam;
mod spawn;
pub mod tint;
pub mod tracer;
pub mod view_model;
pub mod vis;
pub mod water;
pub use spawn::{Drawn, MapAssets};

/// What `map::unload_map` removes of render's: its resources and the
/// entities it made (a `map::MapHooks::unload` hook, added by
/// `map_plugin`).
pub(crate) fn unload_map(world: &mut World) {
    world.remove_resource::<light_styles::StyledLightmaps>();
    world.remove_resource::<decal::DecalSurfaces>();
    world.remove_resource::<decal::DecalAssets>();
    world.remove_resource::<decal::ImpactDecals>();
    world.remove_resource::<psys::ParticleSystemDefs>();
    world.remove_resource::<monitor::MonitorScreen>();
    world.remove_resource::<water::MapWaterRender>();
    world.remove_resource::<sky::MapSkybox>();
    world.remove_resource::<sky::SkyCameraInfo>();
    world.remove_resource::<sky::ActiveMapHas3dSky>();
    world.remove_resource::<sky::SkyVis>();
    world.remove_resource::<shadows::ShadowState>();
    world.remove_resource::<shadows::ShadowSource>();
    world.remove_resource::<particles::ParticleAssets>();
    world.remove_resource::<bodies::CharacterBodies>();
    world.remove_resource::<bodies::HeldAssets>();
    let drawn: Vec<Entity> = world
        .query_filtered::<Entity, With<bodies::CharacterBody>>()
        .iter(world)
        .collect();
    for b in drawn {
        world.entity_mut(b).despawn();
    }
    view_model::unload(world);
    shells::unload(world);
}
