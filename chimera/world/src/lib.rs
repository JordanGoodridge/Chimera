//! The engine's map (docs/ARCHITECTURE.md, "Crates"): the neutral map data
//! any game's importer produces, collision, spawning a map's simulated
//! parts and the map's systems (`map`). It uses only the engine's base
//! (`chimera_core`) and the formats of ours; what draws and plays the map
//! (`chimera_present`) sits above it, so an edit to drawing doesn't
//! rebuild the simulation's map, and the other way round.
//!
//! `map::MapSimPlugin` registers the map's systems alone (the dedicated
//! server's); `chimera_present`'s `MapPlugin` registers the same with what
//! draws and plays the map at fixed places among them (`map::build`,
//! `map::MapLayers`). The game crate re-exports `map` at its old path
//! (`mashup::map`), with `MapPlugin` when it has the presentation.
//!
//! `music`: a background-music player with two stream slots and fades (pure
//! state on a wall clock); `greybox`: the map built in code (tests, the game without a map);
//! `harness`: the headless `Sim` tests and tools step tick by tick, on
//! whatever simulation plugins its caller passes as the base (a game's,
//! or the engine's alone), so each crate's tests use it at their layer.

pub mod greybox;
pub mod harness;
pub mod map;
pub mod music;

// `crate::core` inside `map`, as in the game crate.
use chimera_core::core;

/// Avian as Chimera runs it: its plugins with Chimera's collision hooks
/// (`map::MapCollisionHooks`) and without transform interpolation (the
/// map eases rendering between ticks itself, `map::interp`; headless,
/// interpolation would make `Transform` show the previous tick). Every
/// app and the test harness (`harness::add_headless_with`) add this one
/// group, so the physics can't differ between them (docs/ARCHITECTURE.md).
pub fn physics_plugins() -> bevy::app::PluginGroupBuilder {
    use avian3d::prelude::*;
    use bevy::app::PluginGroup as _;
    PhysicsPlugins::default()
        .with_collision_hooks::<map::MapCollisionHooks>()
        .build()
        .disable::<PhysicsInterpolationPlugin>()
}
