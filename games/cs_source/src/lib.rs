//! Counter-Strike: Source as a game crate (docs/ARCHITECTURE.md,
//! "Crates"): its simulation on the engine's crates and, with the `ui`
//! feature, its front end (`ui`: the GameUI menus and dialogs, the join
//! flow, the buy, team, radio and spectator menus, the scoreboard and
//! CS:S's HUD, on the client shell, `chimera_client`). `game` is the
//! game's parts: its loaders (BSP, materials, models,
//! sounds, the install's search path), Source movement, the weapons and
//! grenades, impacts and effects, animation and `CsSourcePlugins`;
//! `rules` (deathmatch, team joining, rounds, the end of a game) and
//! `objectives` (bomb, hostages, zones) are CS:S's rules, which its rule
//! modules (`game::rules`) describe; `bot` is CS:S's bots (aim,
//! navigation, team plans, radio, buying, grenades, the bomb and
//! hostage goals); `game::net` is its part of the network (its round, bomb
//! and hostages replicated, its join flow, bots and holds behind the
//! network's hooks). It uses the engine crates
//! (`chimera_core`, `chimera_world`, `chimera_sim`, `chimera_net`) and `source_formats`,
//! never `mashup` or another game (checked); only `ui` uses the
//! presentation, the shell and Source's logic (optional dependencies of
//! the feature, checked), so a dedicated server builds and runs the game
//! without drawing and nothing but `ui` uses `ui`.
//!
//! The game crate re-exports `game` as `mashup::games::cs_source` with
//! `ui` beside it (with `present`), and `rules`, `objectives` and `bot` at
//! their old paths (`mashup::rules`, ...).

pub mod bot;
pub mod game;
pub mod objectives;
pub mod rules;
// The front end: built on the client shell (the `ui` feature).
#[cfg(feature = "ui")]
pub mod ui;

// `crate::core`, `crate::map` and the rest inside, as in the game crate.
use chimera_core::{character, console, core, game_registry, mount, slots};
use chimera_sim::weapon;
use chimera_net::net;
use chimera_world::map;

// The front end's: the shell, the presentation, Source's logic.
#[cfg(feature = "ui")]
use chimera_client::client;
#[cfg(feature = "ui")]
use chimera_present::{audio, render};
#[cfg(feature = "ui")]
use source_logic::logic;
