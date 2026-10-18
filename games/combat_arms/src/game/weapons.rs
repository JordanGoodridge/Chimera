//! Combat Arms' firing: how its guns shoot (fire rates, accuracy and
//! spread, recoil, damage by distance and body part, reloads) on
//! Chimera's weapon frame (`weapon`: trigger, magazine, hitscan, melee,
//! damage parts). Defined by specs/combat_arms/weapon_firing.md and, for
//! the classes' differences (scopes, shotguns), weapon_classes.md (not
//! written yet).
//!
//! Wave 0: no behaviour of Combat Arms' own. `CaWeaponsPlugin` registers
//! the data's weapons (`data::weapon_set`: a placeholder rifle and knife)
//! as Chimera builds them, with no hooks: the rifle is dead on, without
//! recoil, penetration, sounds or models.
//!
//! The seam: what the data can't say becomes hooks registered here
//! (`register_weapon_hook`, named `combat_arms:<what>` as CS:S's
//! `cs_source:accuracy`/`recoil`), with their systems in
//! `core::Predict::Weapons` around `weapon::WeaponFrame`; the numbers go
//! into the data files (`data`). Slots and switching are `inventory`'s,
//! thrown weapons `grenades`', attachments `attachments`'.

use bevy::prelude::*;

use crate::weapon::data::WeaponDataAppExt;

/// The placeholder rifle (PLACEHOLDER: data/weapons/placeholder_rifle.toml).
pub const RIFLE: &str = "combat_arms:placeholder_rifle";
/// The placeholder knife (PLACEHOLDER: data/weapons/placeholder_knife.toml).
pub const KNIFE: &str = "combat_arms:placeholder_knife";

/// Combat Arms' weapons registered from its data (`data::weapon_set`).
pub struct CaWeaponsPlugin;

impl Plugin for CaWeaponsPlugin {
    fn build(&self, app: &mut App) {
        if let Err(e) = app.add_weapon_set(super::data::weapon_set()) {
            panic!("Combat Arms weapon data: {e}");
        }
    }
}
