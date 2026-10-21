//! Combat Arms as a game crate (docs/ARCHITECTURE.md, "Crates"): wave 0,
//! the skeleton (docs/plans/active/combat-arms.md). Each part is a
//! placeholder on Chimera's own pieces until its spec
//! (specs/combat_arms/<name>.md, written in a separate spec session;
//! implementation sessions read specs only) is implemented: `inventory`
//! is (inventory.md).
//!
//! `game` is the game's parts, one module per subsystem so parallel
//! implementation work never edits the same file: `movement`, `weapons`
//! (firing), `inventory` (slots, switching, the knife), `grenades`,
//! `attachments`, `data` (its data files and, later, the install's
//! attribute files), `rules` (game modes: TDM, Elimination Pro) and
//! `world` (the install's
//! maps); `CombatArmsPlugins` adds
//! them all. With the `ui` feature, `ui` is its front end on the client
//! shell: `ui::hud` (crosshair and basic HUD) and `ui::visuals`
//! (placeholder box guns). `harness::CaSim` is the simulation as the
//! headless harness's base (its scenario tests, tests/it/).
//!
//! It uses the engine crates (`chimera_core`, `chimera_world`,
//! `chimera_sim`, `chimera_net`), never `mashup`, Source's map logic or another game
//! (checked: checks/tests/it/architecture.rs); only `ui` uses the
//! presentation and the shell (optional dependencies of the feature).
//!
//! `SimPlugins`: everything Combat Arms' simulation needs but its parts
//! (the engine's base, `chimera_sim::SimBasePlugins`, and the tick
//! metrics); the app runs it with `game::CombatArmsPlugins` after it.
//!
//! Bots (bots.md) are `game::bots`; their Known shortcuts are there.

pub mod game;
pub mod harness;
// The front end: built on the client shell (the `ui` feature).
#[cfg(feature = "ui")]
pub mod ui;

// `crate::core`, `crate::weapon` and the rest inside, as in CS:S's crate
// (the architecture check reads `crate::<module>` against its layering
// table).
use chimera_core::{character, console, core, game_registry, mount, profile, slots};
use chimera_net::net;
use chimera_sim::{rule_module, spawning, weapon};
use chimera_world::{map, music};

// The front end's: the shell.
#[cfg(feature = "ui")]
use chimera_client::client;

use bevy::prelude::*;

/// Combat Arms' simulation but its parts, with no window, rendering or
/// input: the engine's base (`chimera_sim::SimBasePlugins`: the fixed
/// tick, `core`, the console, movement, the weapon framework) and the
/// tick metrics. The app runs it with `game::CombatArmsPlugins` after it;
/// tests through `harness::CaSim`.
pub struct SimPlugins;

impl Plugin for SimPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins(chimera_sim::SimBasePlugins)
            .add_plugins(chimera_core::metrics::TickMetricsPlugin);
    }
}
