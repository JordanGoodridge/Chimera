//! Mashup prototype. Module layering is documented in docs/ARCHITECTURE.md
//! and enforced by tests/it/architecture.rs.
//!
//! `SimPlugins` (everything the simulation needs), `swap_map` (a map in place
//! of the one playing, without the view: the dedicated server's `changelevel`,
//! tests).
//!
//! `single_threaded_schedules`: the main world's schedules on Bevy's
//! single-threaded executor (the game, the dedicated server, `perfgate`).

pub mod bot;
pub mod client;
pub mod core;
pub mod games;
pub mod greybox;
pub mod harness;
pub mod logic;
pub mod mount;
pub mod movement;
pub mod net;
pub mod objectives;
pub mod rules;
pub mod weapon;

// The engine's stable base (engine/core), at its old paths; `core` is
// a module of its own here (it adds the build's `VERSION`).
pub use engine_core::{character, console, metrics, slots};

/// The engine's map (engine/world, crate `engine_world`) at its old path,
/// with `MapPlugin` (engine/present's `map_plugin`: its registration
/// includes `audio` and `render`).
pub mod map {
    pub use engine_present::MapPlugin;
    pub use engine_world::map::*;
}

// The engine's presentation (engine/present), at its old paths.
pub use engine_present::{audio, render};

use bevy::prelude::*;

/// Default fixed tick rate until a ruleset or movement spec sets one.
pub const DEFAULT_TICK_HZ: f64 = 64.0;

/// Everything the simulation needs, with no window, rendering or input.
/// Used by the game, headless tests and (later) a dedicated server.
pub struct SimPlugins;

impl Plugin for SimPlugins {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(DEFAULT_TICK_HZ))
            .add_plugins((core::CorePlugin, console::ConsolePlugin, movement::MovementPlugins))
            .add_plugins((
                weapon::WeaponPlugin,
                objectives::ObjectivesPlugin,
                rules::DeathmatchPlugin,
                bot::BotPlugin,
            ))
            .add_plugins((logic::LogicPlugin, metrics::TickMetricsPlugin));
    }
}

/// Run the main world's per-frame and fixed schedules on one thread each
/// instead of Bevy's multi-threaded executor: the game (`main.rs`, which
/// does the render world's too), the dedicated server and `perfgate`.
///
/// A tick runs a few hundred systems, nearly all tiny: handing each to a
/// worker thread and waking the next cost more than the systems did, and
/// much more on a busy machine (docs/performance.md, "Frame time pass";
/// a 32-player server spent half its CPU time on it: multiplayer.md,
/// "Capacity"). Systems that iterate many entities still spread over the
/// task pool themselves.
pub fn single_threaded_schedules(app: &mut App) {
    use bevy::ecs::{
        intern::Interned,
        schedule::{ScheduleLabel, SingleThreadedExecutor},
    };
    let labels: [Interned<dyn ScheduleLabel>; 10] = [
        First.intern(),
        PreUpdate.intern(),
        Update.intern(),
        PostUpdate.intern(),
        Last.intern(),
        FixedFirst.intern(),
        FixedPreUpdate.intern(),
        FixedUpdate.intern(),
        FixedPostUpdate.intern(),
        FixedLast.intern(),
    ];
    for label in labels {
        app.edit_schedule(label, |s| {
            s.set_executor(SingleThreadedExecutor::new());
        });
    }
}

/// Put a map in place of the one playing, as the game's `map` does
/// without the view (the dedicated server's `changelevel`, tests): `data`
/// for map `id`, or the greybox (None); the tick its game runs at;
/// everyone respawned in a new game there. The fixed clock keeps running
/// (`core::set_tick_length`); a network server takes its clients along
/// (`net::maps`).
pub fn swap_map(world: &mut World, id: &str, data: Option<map::MapData>, tick: std::time::Duration) {
    match data {
        Some(data) => {
            greybox::unload(world);
            map::change_map(world, data, map::MapDebugView::Normal);
            world.insert_resource(map::LoadedMapName(id.to_string()));
        }
        None => {
            map::unload_map(world);
            greybox::unload(world);
            greybox::respawn(world);
            world.remove_resource::<map::ActiveMapLook>();
            world.insert_resource(map::LoadedMapName(net::GREYBOX.into()));
        }
    }
    core::set_tick_length(world, tick);
    rules::new_game(world);
}
