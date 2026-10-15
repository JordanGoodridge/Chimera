//! The simulation's frameworks (docs/ARCHITECTURE.md, "Crates"): the
//! weapon system (`weapon`: weapons as parts, inventory, the weapon frame,
//! economy, grenades, drops, lag compensation, sprays), the movement
//! slot's own implementations (`movement`: placeholder walking, noclip)
//! and the rule-module interface (`rule_module`: a game registers its
//! modes by name). A game fills them with its data and hooks; they name
//! no game. It uses the engine's base (`chimera_core`) and the
//! simulation's map (`chimera_world`) and nothing above them, so an edit in
//! a game or the client doesn't rebuild it.
//!
//! The game crate re-exports `weapon` and `movement` at their old paths
//! (`mashup::weapon`, `mashup::movement`) and `rule_module` as
//! `mashup::rules::module`.

pub mod movement;
pub mod rule_module;
pub mod weapon;

// `crate::core`, `crate::map` and the rest inside, as in the game crate.
use chimera_core::{character, console, core, slots};
use chimera_world::map;
