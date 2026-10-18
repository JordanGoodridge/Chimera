//! Combat Arms' inventory: which slots a player has and what goes in each,
//! what a player starts with (its loadout), switching between weapons
//! (keys, wheel, last weapon, switch times) and the knife. Defined by
//! specs/combat_arms/inventory.md (not written yet).
//!
//! Wave 0: Chimera's inventory as it is (`weapon::Inventory`: a slot key
//! picks the first weapon in that slot, `lastinv` the previous one, the
//! wheel steps through them; `Weapon::draw_time` before the first
//! attack), and every player starts with the placeholder knife and rifle,
//! the rifle drawn (`weapon::StartingWeapons`: the last given is drawn).
//! The slot numbers are the data's (PLACEHOLDER: rifle 0, knife 2).
//!
//! The seam: Combat Arms' loadouts (what a player picked before a match)
//! replace `StartingWeapons` here; switching rules Chimera's selection
//! can't express (its own switch times, a quick-knife key) become a part
//! or a hook on the weapons, registered here, with a system before
//! `weapon::SelectWeapons`. A HUD weapon selection is the front end's
//! (`ui::hud`).

use bevy::prelude::*;

use super::weapons::{KNIFE, RIFLE};
use crate::weapon::StartingWeapons;

/// What every Combat Arms player starts with, in giving order (the last
/// is drawn). PLACEHOLDER: the skeleton's knife and rifle.
pub const STARTING_WEAPONS: &[&str] = &[KNIFE, RIFLE];

/// Combat Arms' inventory: the starting weapons, unless the app set some
/// first (a test's).
pub struct CaInventoryPlugin;

impl Plugin for CaInventoryPlugin {
    fn build(&self, app: &mut App) {
        let set = app.world().get_resource::<StartingWeapons>().is_some_and(|s| !s.is_empty());
        if !set {
            app.insert_resource(StartingWeapons {
                team: Vec::new(),
                all: STARTING_WEAPONS.to_vec(),
            });
        }
    }
}
