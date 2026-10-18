//! Combat Arms' thrown weapons: grenades and whatever else is thrown,
//! their throw, flight, fuse and effects. Defined by
//! specs/combat_arms/grenades.md (not written yet).
//!
//! Wave 0: none. Players carry no grenade.
//!
//! The seam: Chimera's thrown-weapon framework (`weapon::grenade`: the
//! throw, the flight and bounces, detonation as `Detonated`) is
//! game-neutral; Combat Arms' grenades are weapon documents in
//! data/weapons (`data`) naming a hook registered here
//! (`register_weapon_hook`, as CS:S's `cs_source:grenade` does with
//! `weapon::grenade` parts), and their effects systems react to
//! detonations here. Blast, flash and smoke effects that CS:S's grenades
//! have in games/cs_source (their reaction to `Detonated`) are lift
//! candidates for Chimera before this module needs them.

use bevy::prelude::*;

/// Combat Arms' grenades: nothing yet.
pub struct CaGrenadesPlugin;

impl Plugin for CaGrenadesPlugin {
    fn build(&self, _app: &mut App) {}
}
