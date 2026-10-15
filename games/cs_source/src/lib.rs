//! Counter-Strike: Source as a game crate (docs/ARCHITECTURE.md,
//! "Crates"): its simulation on the engine's crates, without its front
//! end. `game` is the game's parts: its loaders (BSP, materials, models,
//! sounds, the install's search path), Source movement, the weapons and
//! grenades, impacts and effects, animation and `CsSourcePlugins`;
//! `rules` (deathmatch, team joining, rounds, the end of a game) and
//! `objectives` (bomb, hostages, zones) are CS:S's rules, which its rule
//! modules (`game::rules`) describe; `bot` is CS:S's bots (aim,
//! navigation, team plans, radio, buying, grenades, the bomb and
//! hostage goals). It uses the engine crates
//! (`engine_core`, `engine_world`, `engine_sim`) and `source_formats`,
//! never the presentation, `mashup` or another game (checked), so a
//! dedicated server runs it without drawing and an edit in the client
//! doesn't rebuild it.
//!
//! The game crate re-exports `game` as `mashup::games::cs_source` (beside
//! the front end, `ui/`, which stays with the client shell for now) and
//! `rules`, `objectives` and `bot` at their old paths (`mashup::rules`,
//! ...). The network and the client still name them
//! (docs/plans/active/engine-split.md, "Remaining couplings").

pub mod bot;
pub mod game;
pub mod objectives;
pub mod rules;

// `crate::core`, `crate::map` and the rest inside, as in the game crate.
use engine_core::{character, console, core, game_registry, mount, slots};
use engine_sim::weapon;
use engine_world::map;
