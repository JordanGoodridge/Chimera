//! The faithful Counter-Strike: Source app's glue (apps/cs_source, crate
//! `cs_source_app`): what composes Chimera, Source's logic and CS:S into
//! the game (the `cs_source` binary, `main.rs`), the dedicated server
//! (apps/cs_source_server), the tools that drive the simulation, and its
//! own integration tests (tests/it/: what only the app has, its build's
//! version). `mashup`, the repository root, re-exports this crate at the
//! old `mashup::` paths. Module layering is documented in
//! docs/ARCHITECTURE.md and enforced by checks/tests/it/architecture.rs.
//!
//! `SimPlugins`: CS:S's simulation (`cs_source::SimPlugins`) with this
//! build's version. At their old paths: `harness` (the engine's headless
//! `Sim` and `NetSim` with CS:S's base and test maps), `greybox`
//! (chimera_world's), `swap_map` (cs_source's) and
//! `single_threaded_schedules` (chimera_world's harness: the game, the
//! dedicated server, `perfgate`).

// The client shell: window, input, camera, HUD; not in the dedicated
// server's build (the `present` feature, Cargo.toml).
#[cfg(feature = "present")]
pub mod client;
pub mod core;
pub mod games;
pub mod net;

/// The headless harness (docs/OBSERVABILITY.md, section 1) at its old
/// path: `Sim` (chimera_world), `NetSim` (chimera_net), CS:S's base
/// `CsSim` and its test maps (`cs_source::harness`).
pub mod harness {
    pub use chimera_net::harness::*;
    pub use chimera_world::harness::*;
    pub use cs_source::harness::*;
}

// The greybox (chimera_world), the map change (cs_source) and the
// executor setup (chimera_world's harness), at their old paths.
pub use chimera_world::{greybox, harness::single_threaded_schedules};
pub use cs_source::swap_map;

// The engine's stable base (chimera/core), at its old paths; `core` is
// a module of its own here (it adds the build's `VERSION`).
pub use chimera_core::{character, console, env, game_log, game_registry, metrics, mount, slots};

/// The engine's map (chimera/world, crate `chimera_world`) at its old path,
/// with `MapPlugin` (chimera/present's `map_plugin`: map's registration
/// with `audio` and `render`) when built with the presentation; the
/// dedicated server adds `MapSimPlugin` (map's systems alone).
pub mod map {
    #[cfg(feature = "present")]
    pub use chimera_present::MapPlugin;
    pub use chimera_world::map::*;
}

// The engine's presentation (chimera/present), at its old paths; not in
// the dedicated server's build (the `present` feature, Cargo.toml).
#[cfg(feature = "present")]
pub use chimera_present::{audio, render};

// Source's map logic (source/logic: entity I/O, logic classes, triggers,
// movers), at its old path.
pub use source_logic::logic;

// The simulation's frameworks (chimera/sim: weapons, the movement
// slot's own implementations), at their old paths.
pub use chimera_sim::{movement, weapon};

// CS:S's rules (games/cs_source: deathmatch, rounds, the bomb and the
// hostages), at their old paths; `games::cs_source` re-exports the rest.
pub use cs_source::{bot, objectives, rules};

use bevy::prelude::*;

/// Default fixed tick rate until a ruleset or movement spec sets one
/// (`core::DEFAULT_TICK_HZ`).
pub use chimera_core::core::DEFAULT_TICK_HZ;

/// Everything the simulation needs, with no window, rendering or input:
/// CS:S's (`cs_source::SimPlugins`) with this build's version
/// (`core::BuildVersion`). The game and the dedicated server run it.
pub struct SimPlugins;

impl Plugin for SimPlugins {
    fn build(&self, app: &mut App) {
        app.insert_resource(core::BuildVersion(core::VERSION))
            .add_plugins(cs_source::SimPlugins);
    }
}

/// What this app tells the client shell (`client::AppHooks`): its commit,
/// how it loads maps (`games::load_map_level`, `load_map_file`,
/// `map_file_bytes`) and its greybox.
#[cfg(feature = "present")]
pub fn client_hooks() -> client::AppHooks {
    client::AppHooks {
        build: core::BUILD,
        load_map: games::load_map_level,
        load_map_file: games::load_map_file,
        map_file_bytes: games::map_file_bytes,
        unload_greybox: greybox::unload,
        respawn_greybox: greybox::respawn,
    }
}
