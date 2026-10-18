//! Combat Arms' movement: the movement slot's implementation for its
//! players (docs/DESIGN.md, "Slots"). Defined by
//! specs/combat_arms/movement.md (not written yet).
//!
//! Wave 0: no movement of Combat Arms' own. Its players walk with
//! Chimera's placeholder (`movement::placeholder`, deliberately no game's
//! movement), which `ID` names and `CaMovementPlugin` makes the loadout's
//! default.
//!
//! The seam: the spec's implementation registers here under
//! `combat_arms:movement` with its marker component
//! (`slots::RegisterSlots::register_movement`, as CS:S's
//! `cs_source:movement`), its step in `core::Predict::Movement`, and `ID`
//! becomes that name; nothing else names the movement's types. Its cvars
//! go in `game::INFO.movement_cvars` for the debug UI.

use bevy::prelude::*;

use crate::{movement::placeholder, slots::Loadout};

/// The movement Combat Arms' players get (PLACEHOLDER: Chimera's stand-in
/// walking until specs/combat_arms/movement.md is implemented).
pub const ID: &str = placeholder::ID;

/// Combat Arms' movement: for now, the loadout's default is `ID`.
pub struct CaMovementPlugin;

impl Plugin for CaMovementPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Loadout { movement: ID });
    }
}
