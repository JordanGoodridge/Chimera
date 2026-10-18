//! Combat Arms' data: the documents built into the game
//! (games/combat_arms/data/, parsed at run time by Chimera's
//! `weapon::data`, docs/weapon-data.md) and, later, what it reads from the
//! user's install (its attribute files: weapons, items, modes). Defined by
//! specs/combat_arms/attribute_data.md (not written yet): which install files hold
//! what, and how they map onto the weapon data's parts and hooks.
//!
//! Two weapons: the M4A1 (data/weapons/m4a1.toml, the numbers of
//! weapon_firing.md's worked example) and the M9 knife (m9.toml,
//! inventory.md's numbers). Nothing is read from the install.
//!
//! The seam: `weapon_set` is every Combat Arms weapon as a parsed
//! `WeaponSet`, built in once per process; the install's attribute files
//! come in here (through `game::INFO.open`'s mount), converted to weapon
//! documents or a `WeaponSet` beside the built-in one. The other modules
//! read data only through this one: `weapons` registers the set,
//! `inventory` names what a player starts with.

use std::sync::OnceLock;

use bevy::prelude::*;

use crate::weapon::data::WeaponSet;

macro_rules! data_file {
    ($name:literal) => {
        ($name, include_str!(concat!("../../data/weapons/", $name)))
    };
}

/// Combat Arms' weapon documents (games/combat_arms/data/weapons), built
/// in, in registration order (the network names weapons by registry
/// index).
pub const WEAPON_DATA: &[(&str, &str)] = &[
    data_file!("m9.toml"),
    data_file!("m4a1.toml"),
];

/// Combat Arms' weapons as data, parsed once per process (and findable by
/// `weapon::data::lookup`). Bad data panics: the tests load it.
pub fn weapon_set() -> &'static WeaponSet {
    static SET: OnceLock<&'static WeaponSet> = OnceLock::new();
    SET.get_or_init(|| {
        let set = WeaponSet::parse(WEAPON_DATA, &[]).unwrap_or_else(|e| panic!("Combat Arms weapon data: {e}"));
        let set: &'static WeaponSet = Box::leak(Box::new(set));
        crate::weapon::data::publish(set);
        set
    })
}

/// Combat Arms' data, loaded as the game starts (so a bad file fails at
/// once, not at the first weapon given).
pub struct CaDataPlugin;

impl Plugin for CaDataPlugin {
    fn build(&self, _app: &mut App) {
        weapon_set();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file in data/weapons is built in (`WEAPON_DATA`), as it is
    /// on disk.
    #[test]
    fn every_data_file_is_built_in() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/weapons");
        let files = WeaponSet::read_dir(&dir).unwrap();
        for (name, text) in &files {
            let built_in = WEAPON_DATA.iter().find(|(n, _)| n == name);
            assert_eq!(built_in.map(|d| d.1), Some(text.as_str()), "{name} not in `WEAPON_DATA`");
        }
        assert_eq!(files.len(), WEAPON_DATA.len());
    }

    /// Every placeholder document says its numbers are placeholders until
    /// the specs replace them.
    #[test]
    fn placeholder_data_says_so() {
        for (name, text) in WEAPON_DATA.iter().filter(|(n, _)| n.starts_with("placeholder_")) {
            assert!(text.contains("PLACEHOLDER"), "{name}: no PLACEHOLDER note");
        }
        assert_eq!(weapon_set().weapons.len(), 2);
    }
}
