//! Combat Arms' data as server documents (chimera_sim's `server_docs`;
//! docs/plans/active/multiplayer.md, "Server data documents"): what a
//! network client takes from the server when its own differs (no `V1`
//! key, another install version), so its registry indices and numbers are
//! the server's.
//!
//! - `WEAPONS`: the weapon documents (`CaDataInner::weapon_docs`: ours
//!   with the install's ~2,300 merged over them, in registry order; the
//!   clip, `FireAniRate`, spread, recoil, alt modes and each copy's
//!   accepted parts are in them). Registered by `weapons`.
//! - `ATTACHMENTS`: the attachment records (`CaModTable`: ours with the
//!   install's modification file over them by name; a record's index is
//!   its id on the network), as one mods.toml-format document
//!   (`attachments_doc`). Registered by `attachments`.
//! - `MOVEMENT`: the movement numbers with the install's body type
//!   (`MovementData`), as one movement.toml-format document. Registered
//!   by `movement`.
//!
//! Known shortcuts: the gear and perk records (`gear::records::GearData`:
//! the vests' movement penalty, armour) aren't sent: they are built from
//! the install's records into maps with no document form to write back
//! (`GearData::parse` reads ours only); a client with other gear data
//! mispredicts a vest's speed (armour itself replicates). Models, skins
//! and sounds the documents name are still read from the client's own
//! install (a missing one: the fallback).

use bevy::prelude::*;
use serde::Serialize;

use super::super::{
    attachments::{CaModTable, ModRecord, parse_mods},
    movement::MovementData,
};

/// The set names.
pub const WEAPONS: &str = "combat_arms:weapons";
pub const ATTACHMENTS: &str = "combat_arms:attachments";
pub const MOVEMENT: &str = "combat_arms:movement";

#[derive(Serialize)]
struct Mods<'a> {
    #[serde(rename = "mod")]
    records: &'a [ModRecord],
}

/// The attachment records as one document (mods.toml's format, names
/// expanded).
pub fn attachments_doc(table: &CaModTable) -> Vec<(String, String)> {
    let text = toml::to_string(&Mods {
        records: &table.records,
    })
    .expect("attachment records serialize");
    vec![("mods.toml".into(), text)]
}

/// The server's attachment records in place of this app's.
pub fn apply_attachments(world: &mut World, docs: &[(String, String)]) -> Result<(), String> {
    let (_, text) = docs.first().ok_or("no attachments document")?;
    let records = parse_mods(text)?;
    world.insert_resource(CaModTable { records });
    Ok(())
}

/// The movement numbers as one document (movement.toml's format).
pub fn movement_doc(data: &MovementData) -> Vec<(String, String)> {
    let text = toml::to_string(data).expect("movement data serializes");
    vec![("movement.toml".into(), text)]
}

/// The server's movement numbers in place of this app's.
pub fn apply_movement(world: &mut World, docs: &[(String, String)]) -> Result<(), String> {
    let (_, text) = docs.first().ok_or("no movement document")?;
    let data: MovementData = toml::from_str(text).map_err(|e| e.to_string())?;
    world.insert_resource(data);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The documents read back as what they were written from.
    #[test]
    fn documents_round_trip() {
        let table = CaModTable::built_in().clone();
        let docs = attachments_doc(&table);
        assert_eq!(parse_mods(&docs[0].1).unwrap(), table.records);
        let m = super::super::super::movement::movement_data().clone();
        let docs = movement_doc(&m);
        let back: MovementData = toml::from_str(&docs[0].1).unwrap();
        assert_eq!(format!("{back:?}"), format!("{m:?}"));
    }
}
